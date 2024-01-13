use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::protocol::WINDOW_SIZE;
use bytes::Bytes;

pub struct SequenceWindow {
    pub start: u32,
    pub end: u32,
    pub highest: u32,
    pub acks: Vec<u32>,
    pub nacks: Vec<u32>,
}

impl SequenceWindow {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            highest: 0,
            acks: Vec::new(),
            nacks: Vec::new(),
        }
    }

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

    pub fn shift(&mut self) {
        self.start += self.highest + 1;
        self.end += self.highest + 1;
    }
}

pub struct MessageWindow {
    pub start: u32,
    pub end: u32,
    pub indexes: Vec<u32>,
}

impl MessageWindow {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: WINDOW_SIZE,
            indexes: Vec::new(),
        }
    }

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

pub struct SplitWindow {
    pub count: u32,
    pub fragments: Vec<Vec<u8>>,
}

impl SplitWindow {
    pub fn new(count: u32) -> Self {
        Self {
            count,
            fragments: Vec::with_capacity(count as usize),
        }
    }

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

pub struct Record {
    packet: Bytes,
    instant: Instant,
}

pub struct RecoveryWindow {
    pub unacknowledged: HashMap<u32, Record>,
    pub delays: HashMap<Instant, Duration>,
}

impl RecoveryWindow {
    pub fn new() -> Self {
        Self {
            unacknowledged: HashMap::new(),
            delays: HashMap::new(),
        }
    }

    pub fn add(&mut self, sequence: u32, packet: Bytes) {
        self.unacknowledged.insert(
            sequence,
            Record {
                packet,
                instant: Instant::now(),
            },
        );
    }

    pub fn acknowledge(&mut self, sequence: u32) {
        if let Some(record) = self.unacknowledged.remove(&sequence) {
            self.delays.insert(Instant::now(), record.instant.elapsed());
        }
    }

    pub fn retransmit(&mut self, sequence: u32) -> Option<Bytes> {
        if let Some(record) = self.unacknowledged.remove(&sequence) {
            self.delays
                .insert(Instant::now(), record.instant.elapsed() * 2);
            return Some(record.packet);
        }

        None
    }

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
