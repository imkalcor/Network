use std::{
    io::{Cursor, Error, ErrorKind, Result, Write},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::{ecs::component::Component, utils::HashMap};
use binary::{
    datatypes::{I16, U16, U24, U32},
    Binary,
};
use byteorder::{ReadBytesExt, BE, LE};
use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::{
    generic::window::{MessageWindow, SequenceWindow, SplitWindow},
    protocol::{
        reliability::Reliability, DATAGRAM_HEADER_SIZE, FLAG_ACK, FLAG_DATAGRAM, FLAG_FRAGMENTED,
        FLAG_NACK, FLAG_NEEDS_B_AND_AS, FRAME_ADDITIONAL_SIZE, FRAME_HEADER_SIZE,
        MAX_BATCHED_PACKETS, MAX_MTU_SIZE, MAX_SPLIT_PACKETS, UDP_HEADER_SIZE,
    },
};

/// Network Information about a connected RakNet client.
#[derive(Component)]
pub struct NetworkInfo {
    pub last_activity: Instant,
    pub latency: Duration,
    pub ping: u64,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
}

/// RakNetEncoder handles the encoding of RakNet datagrams, ACKs, NACKs and sends them over the wire.
/// It also supports batching of outgoing datagrams to ensure network efficiency.
#[derive(Component)]
pub struct RakNetEncoder {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    mtu_size: usize,

    sequence_number: u32,
    message_index: u32,
    sequence_index: u32,
    order_index: u32,
    split_id: u16,

    buffer: BytesMut,
}

impl RakNetEncoder {
    /// Creates and returns a new RakNetEncoder.
    pub fn new(addr: SocketAddr, socket: Arc<UdpSocket>, mtu_size: u16) -> Self {
        Self {
            addr,
            socket,
            mtu_size: mtu_size as usize,
            sequence_number: 0,
            message_index: 0,
            sequence_index: 0,
            order_index: 0,
            split_id: 0,
            buffer: BytesMut::with_capacity(MAX_MTU_SIZE),
        }
    }

    /// Encodes the provided encoded raknet message on the wire with the specified reliability.
    pub fn encode(&mut self, bytes: &[u8], reliability: Reliability) {
        let fragments = self.split(&bytes);

        let order_index = self.order_index;
        self.order_index += 1;

        let split_count = fragments.len() as u32;
        let split_id = self.split_id;
        let split = split_count > 0;

        if split {
            self.split_id += 1;
        }

        for split_index in 0..split_count {
            let content = fragments[split_index as usize];
            let len = (content.len() as u16) << 3;

            let mut max_len = self.mtu_size - &self.buffer.len() - DATAGRAM_HEADER_SIZE;
            if split {
                max_len -= FRAME_ADDITIONAL_SIZE;
            }

            if content.len() > max_len || reliability != Reliability::ReliableOrdered {
                self.flush();
            }

            let mut header = (reliability.clone() as u8) << 5;
            if split {
                header |= FLAG_FRAGMENTED;
            }

            self.buffer.put_u8(header);
            self.buffer.put_u16(len);

            if reliability.reliable() {
                U24::<LE>::new(self.message_index).serialize(&mut self.buffer);
                self.message_index += 1;
            }

            if reliability.sequenced() {
                U24::<LE>::new(self.sequence_index).serialize(&mut self.buffer);
                self.sequence_index += 1;
            }

            if reliability.sequenced_or_ordered() {
                U24::<LE>::new(order_index).serialize(&mut self.buffer);
            }

            if split {
                self.buffer.put_u32(split_count);
                self.buffer.put_u16(split_id);
                self.buffer.put_u32(split_index);
            }

            self.buffer.write_all(&content).unwrap();
        }
    }

    /// Splits the raknet message into multiple fragments if it exceeds the maximum size of a datagram.
    fn split<'a>(&mut self, bytes: &'a [u8]) -> Vec<&'a [u8]> {
        let mut max_size =
            self.mtu_size - UDP_HEADER_SIZE - DATAGRAM_HEADER_SIZE - FRAME_HEADER_SIZE;

        let len = bytes.len();

        if len > max_size {
            max_size -= FRAME_ADDITIONAL_SIZE;
        }

        let mut count = len / max_size;
        if len % max_size != 0 {
            count += 1;
        }

        let mut fragments = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * max_size;
            let mut end = start + max_size;

            if end > len {
                end = len;
            }

            fragments.insert(i, &bytes[start..end]);
        }

        fragments
    }

    /// Flushes the current buffer written over the wire and resets the essentials.
    fn flush(&mut self) {
        let mut header = [0u8; 4];
        let mut slice = header.as_mut_slice();
        slice.put_u8(FLAG_DATAGRAM | FLAG_NEEDS_B_AND_AS);
        U24::<LE>::new(self.sequence_number).serialize(&mut slice);

        self.sequence_number += 1;
        let buffer: &[u8] = &[&header[..], &self.buffer[..]].concat();

        self.socket.send_to(&buffer, self.addr).unwrap();
        self.buffer.clear();
    }
}

#[derive(Component)]
pub struct RakNetDecoder {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,

    sequence_window: SequenceWindow,
    message_window: MessageWindow,
    splits: HashMap<u16, SplitWindow>,

    acks: Vec<u32>,
    ack_buf: BytesMut,
}

impl RakNetDecoder {
    pub fn decode(
        &mut self,
        buffer: &[u8],
        network: &mut NetworkInfo,
    ) -> Result<Option<Vec<Bytes>>> {
        let mut reader = Cursor::new(buffer);
        let header = reader.read_u8()?;

        if header & FLAG_DATAGRAM == 0 {
            return Err(Error::new(
                ErrorKind::Other,
                "Buffer does not have a valid FLAG_DATAGRAM",
            ));
        }

        network.last_activity = Instant::now();

        if header & FLAG_ACK != 0 {
            self.handle_ack(false, &mut reader)?;
            return Ok(None);
        }

        if header & FLAG_NACK != 0 {
            self.handle_ack(true, &mut reader)?;
            return Ok(None);
        }

        self.handle_datagram(&mut reader)
    }

    fn handle_ack(&mut self, nack: bool, reader: &mut Cursor<&[u8]>) -> Result<()> {
        let record_count = I16::<BE>::deserialize(reader)?.0;

        for _ in 0..record_count {
            let record_type = reader.read_u8()?;

            match record_type {
                0 => {
                    let start = U24::<LE>::deserialize(reader)?.0;
                    let end = U24::<LE>::deserialize(reader)?.0;

                    for seq in start..end {
                        self.acks.push(seq);
                    }
                }
                1 => {
                    let seq = U24::<LE>::deserialize(reader)?.0;
                    self.acks.push(seq);
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::Other,
                        "Record Type can either be Single (1) or Range (0)",
                    ));
                }
            }
        }

        Ok(())
    }

    fn handle_datagram(&mut self, reader: &mut Cursor<&[u8]>) -> Result<Option<Vec<Bytes>>> {
        let seq = U24::<LE>::deserialize(reader)?.0;

        if !self.sequence_window.receive(seq) {
            return Ok(None);
        }

        let mut messages = Vec::with_capacity(MAX_BATCHED_PACKETS);

        while reader.remaining() != 0 {
            let header = reader.read_u8()?;
            let split = (header & FLAG_FRAGMENTED) != 0;
            let reliability = Reliability::try_from((header & 224) >> 5)?;

            let mut length = U16::<BE>::deserialize(reader)?.0;
            length >>= 3;

            if length == 0 {
                return Err(Error::new(ErrorKind::Other, "Game Bytes size cannot be 0"));
            }

            let mut message_index = 0;

            if reliability.reliable() {
                message_index = U24::<LE>::deserialize(reader)?.0;
            }

            if reliability.sequenced() {
                reader.advance(3);
            }

            if reliability.sequenced_or_ordered() {
                reader.advance(4); // order index & order channel; we don't care about this
            }

            let mut split_count = 0;
            let mut split_id = 0;
            let mut split_index = 0;

            if split {
                split_count = U32::<BE>::deserialize(reader)?.0;
                split_id = U16::<BE>::deserialize(reader)?.0;
                split_index = U32::<BE>::deserialize(reader)?.0;
            }

            let start = reader.position() as usize;
            let end = start + length as usize;

            let content = reader.get_ref()[start..end].to_vec();

            if !self.message_window.receive(message_index) {
                continue;
            }

            if split {
                if split_count >= MAX_SPLIT_PACKETS {
                    return Err(Error::new(
                        ErrorKind::Other,
                        "Maximum number of split packets reached",
                    ));
                }

                let mut splits = self
                    .splits
                    .remove(&split_id)
                    .unwrap_or(SplitWindow::new(split_count));

                if let Some(buffer) = splits.receive(split_index, content) {
                    messages.push(buffer);
                }
            }

            let content = reader.get_ref()[start..end].to_vec();
            messages.push(Bytes::from(content));
        }

        Ok(Some(messages))
    }

    /// Flushes all the ACKs/NACKs for the sequences we have received and not received respectively to the
    /// socket connection.
    fn flush_acks(&mut self) {
        self.encode_ack(false);
        self.encode_ack(true);
    }

    /// Encodes an acknowledgement packet for the provided sequences to the socket.
    fn encode_ack(&mut self, nack: bool) {
        let sequences = if !nack {
            self.sequence_window.acks.as_mut_slice()
        } else {
            self.sequence_window.nacks.as_mut_slice()
        };

        sequences.sort();

        let flag = if nack { FLAG_NACK } else { FLAG_ACK };

        self.ack_buf.put_u8(flag | FLAG_DATAGRAM);
        self.ack_buf.put_i16(0);

        let mut first = sequences[0];
        let mut last = sequences[1];

        let mut record_count = 0;

        for index in 0..sequences.len() {
            let packet = sequences[index];

            if packet == last + 1 {
                last = packet;

                if index != sequences.len() - 1 {
                    continue;
                }
            }

            if first == last {
                self.ack_buf.put_u8(1);
                U24::<LE>::new(first).serialize(&mut self.ack_buf);
            } else {
                self.ack_buf.put_u8(0);
                U24::<LE>::new(first).serialize(&mut self.ack_buf);
                U24::<LE>::new(last).serialize(&mut self.ack_buf);
            }

            first = packet;
            last = packet;
            record_count += 1;
        }

        let mut reserved = &mut self.ack_buf[1..3];
        reserved.put_i16(record_count);

        self.socket.send_to(&self.ack_buf, self.addr).unwrap();
        self.ack_buf.clear();
    }
}

#[derive(Component)]
pub struct NetworkEncoder {}

#[derive(Component)]
pub struct NetworkDecoder {}
