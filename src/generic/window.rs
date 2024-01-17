use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::protocol::WINDOW_SIZE;
use bytes::Bytes;

/// SequenceWindow helps in filtering the incoming RakNet datagrams by preventing any datagrams that have
/// same sequence number or are out of order from reaching our processing side. It maintains a list of acks
/// and nacks that we should flush by the next tick for the sequences we have received and for those we did
/// not respectively.
pub struct SequenceWindow {
    pub start: u32,
    pub end: u32,
    pub highest: u32,
    pub acks: Vec<u32>,
    pub nacks: Vec<u32>,
}

impl SequenceWindow {
    /// Creates and returns a new Sequence Window.
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            highest: 0,
            acks: Vec::with_capacity(WINDOW_SIZE as usize),
            nacks: Vec::with_capacity(WINDOW_SIZE as usize),
        }
    }

    /// Receives a sequence number and checks if we have received this sequence before or
    /// if it is out of order. It returns true if we should continue processing this datagram.
    pub fn receive(&mut self, seq: u32) -> bool {
        if seq < self.start || seq > self.end || self.acks.contains(&seq) {
            return false;
        }

        self.nacks.retain(|&x| x != seq);
        self.acks.push(seq);

        if seq > self.highest {
            self.highest = seq;
        }

        if seq == self.start {
            for i in self.start..self.end {
                if !self.acks.contains(&i) {
                    break;
                }

                self.start += 1;
                self.end += 1;
            }
        } else {
            for i in self.start..seq {
                if !self.acks.contains(&i) {
                    self.nacks.push(i);
                }
            }
        }

        true
    }

    /// Shifts the window, this should be called when we a RakNet tick has passed and we should
    /// stop expecting a certain set of sequences. At this stage, we flush our ACKs and NACKs.
    pub fn shift(&mut self) {
        self.start += self.highest + 1;
        self.end += self.highest + 1;
    }
}

/// MessageWindow ensures that no datagrams with same message index can reach our processing end. This
/// is a second shield from ensuring we don't accidentally handle retransmitted or duplicated datagrams.
/// RakNet Datagrams can have unique sequence numbers and have same message index sometimes due to having being
/// retransmitted by the other end of the connection if they don't receive ACK or NACK for that sequence within
/// a certain period of time.
pub struct MessageWindow {
    pub start: u32,
    pub end: u32,
    pub indexes: Vec<u32>,
}

impl MessageWindow {
    /// Creates and returns a new Message Window.
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            indexes: Vec::with_capacity(WINDOW_SIZE as usize),
        }
    }

    /// Tries to receive a message index and returns whether we should continue processing this datagram or not.
    /// Returns false if a datagram with the provided message index has already reached us before.
    pub fn receive(&mut self, index: u32) -> bool {
        if index < self.start || index > self.end || self.indexes.contains(&index) {
            return false;
        }

        self.indexes.push(index);

        if index == self.start {
            for i in self.start..self.end {
                if !self.indexes.contains(&i) {
                    break;
                }

                self.indexes.retain(|&x| x != i);
                self.start += 1;
                self.end += 1;
            }
        }

        true
    }
}

/// SplitWindow ensures that all the datagrams that are fragmented by the other end of the connection are
/// unsplit to form a fully encapsulated datagram so it can be processed further like the unsplit datagrams.
pub struct SplitWindow {
    pub count: u32,
    pub fragments: Vec<Vec<u8>>,
}

impl SplitWindow {
    /// Creates a new SplitWindow and returns it.
    pub fn new(count: u32) -> Self {
        Self {
            count,
            fragments: Vec::with_capacity(count as usize),
        }
    }

    /// Tries to receive a fragment. Returns optionally fully encapsulated datagram packet if
    /// all the fragments have been received.
    pub fn receive(&mut self, index: u32, fragment: Vec<u8>) -> Option<Vec<u8>> {
        self.fragments.insert(index as usize, fragment);

        if self.fragments.capacity() != self.fragments.len() {
            return None;
        }

        let mut buffer = self.fragments.remove(0);

        for i in 1..self.fragments.len() {
            let fragment = self.fragments.remove(i);
            buffer.extend_from_slice(&fragment);
        }

        Some(buffer)
    }
}

/// Record contains information about the datagram that we have sent to the other end of the
/// connection. It contains the time at which we sent the datagram which is useful for calculating
/// latency, and also contains the encoded bytes that will be useful when retransmitting this datagram.
pub struct Record {
    packet: Bytes,
    instant: Instant,
}

/// RecoveryWindow helps in retransmission of datagrams that the other end of the connection ended up not having
/// or for those datagrams that were arrived late and by that time they already sent a NACK for that sequence to us.
/// Retransmission also occurs from our end if we don't receive an ACK or a NACK for a certain amount of time.
pub struct RecoveryWindow {
    pub unacknowledged: HashMap<u32, Record>,
    pub delays: HashMap<Instant, Duration>,
}

impl RecoveryWindow {
    /// Creates and returns a new Recovery Window.
    pub fn new() -> Self {
        Self {
            unacknowledged: HashMap::new(),
            delays: HashMap::new(),
        }
    }

    /// Adds the datagram to the Recovery Window.
    pub fn add(&mut self, sequence: u32, packet: Bytes) {
        self.unacknowledged.insert(
            sequence,
            Record {
                packet,
                instant: Instant::now(),
            },
        );
    }

    /// Removes the datagram from the recovery window.
    pub fn acknowledge(&mut self, sequence: u32) {
        if let Some(record) = self.unacknowledged.remove(&sequence) {
            self.delays.insert(Instant::now(), record.instant.elapsed());
        }
    }

    /// Returns the datagram encoded bytes if the datagram with the provided sequence
    /// exists in the recovery queue.
    pub fn retransmit(&mut self, sequence: u32) -> Option<Bytes> {
        if let Some(record) = self.unacknowledged.remove(&sequence) {
            self.delays
                .insert(Instant::now(), record.instant.elapsed() * 2);
            return Some(record.packet);
        }

        None
    }

    /// Returns the average time taken by the other end of the connection to acknowledge or NACK
    /// a sequence. This is also known as latency.
    pub fn rtt(&mut self) -> Duration {
        let mut total = Duration::from_secs(0);
        let mut records = 0;

        self.delays.retain(|&time, _| time.elapsed().as_secs() <= 5);

        for (_, duration) in self.delays.iter() {
            total += *duration;
            records += 1;
        }

        if records != 0 {
            return total / records;
        }

        Duration::from_secs(0)
    }
}
