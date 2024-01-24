use std::{
    collections::{HashMap, VecDeque},
    io::{Cursor, Error, ErrorKind, Result, Write},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::ecs::{bundle::Bundle, component::Component, entity::Entity, event::EventWriter};
use binary::{
    datatypes::{I16, I64, U16, U24, U32},
    Binary,
};
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use bytes::{Buf, BufMut, BytesMut};
use commons::utils::unix_timestamp;
use log::{info, trace};

use crate::{
    generic::{
        events::RakNetEvent,
        window::{MessageWindow, RecoveryWindow, SequenceWindow, SplitWindow},
    },
    protocol::{
        binary::{SystemAddresses, UDPAddress},
        message::Message,
        reliability::Reliability,
        DATAGRAM_HEADER_SIZE, FLAG_ACK, FLAG_DATAGRAM, FLAG_FRAGMENTED, FLAG_NACK,
        FLAG_NEEDS_B_AND_AS, FRAME_ADDITIONAL_SIZE, FRAME_HEADER_SIZE, LOGIN_PACKET_ID,
        MAX_BATCHED_PACKETS, MAX_MESSAGE_SIZE, MAX_MTU_SIZE, MAX_RECEIPT_SIZE, MAX_SPLIT_PACKETS,
        UDP_HEADER_SIZE,
    },
};

/// StreamBundle contains components that are required to be spawned for an entity representing
/// an established RakNet connection.
#[derive(Bundle)]
pub struct StreamBundle {
    pub info: NetworkInfo,
    pub status: NetworkStatus,
    pub rakstream: RakStream,
}

/// NetworkInfo contains the local and the remote address of the established RakNet Connection.
#[derive(Component)]
pub struct NetworkInfo {
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
}

/// NetworkStatus contains the current status information of the network such as the ping, latency or last activity
/// of the other end of the connection.
#[derive(Component)]
pub struct NetworkStatus {
    pub ping: u64,
    pub latency: Duration,
    pub last_activity: Instant,
}

/// RakStream represents a component that handles reliable encoding and decoding of messages, receiepts from the
/// other end of the connection.
#[derive(Component)]
pub struct RakStream {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    mtu_size: usize,

    sequence_number: u32,
    message_index: u32,
    sequence_index: u32,
    order_index: u32,
    split_id: u16,

    sequence_window: SequenceWindow,
    message_window: MessageWindow,
    split_window: HashMap<u16, SplitWindow>,
    recovery_window: RecoveryWindow,

    receipts: VecDeque<u32>,

    receiptbuf: BytesMut,
    msgbuf: BytesMut,
    buffer: BytesMut,
}

impl RakStream {
    /// Creates and returns a new RakStream.
    pub fn new(addr: SocketAddr, socket: Arc<UdpSocket>, mtu_size: usize) -> Self {
        Self {
            addr,
            socket,
            mtu_size,
            sequence_number: 0,
            message_index: 0,
            sequence_index: 0,
            order_index: 0,
            split_id: 0,
            sequence_window: SequenceWindow::new(),
            message_window: MessageWindow::new(),
            split_window: HashMap::new(),
            recovery_window: RecoveryWindow::new(),
            receipts: VecDeque::new(),
            receiptbuf: BytesMut::with_capacity(MAX_RECEIPT_SIZE),
            msgbuf: BytesMut::with_capacity(MAX_MESSAGE_SIZE),
            buffer: BytesMut::with_capacity(MAX_MTU_SIZE),
        }
    }

    /// Encodes the provided message with the specified Reliability and batches it for transmission
    /// to the other end of the connection whenever possible.
    pub fn encode(&mut self, message: Message, reliability: Reliability) {
        message.serialize(&mut self.msgbuf);
        let fragments = self.split(&self.msgbuf);

        let order_index = self.order_index;
        self.order_index += 1;

        let split_count = fragments.len() as u32;
        let split_id = self.split_id;
        let split = split_count > 1;

        if split {
            self.split_id += 1;
        }

        for split_index in 0..split_count {
            let content = fragments[split_index as usize];
            let max_len = self.buffer.capacity() - self.buffer.len() - FRAME_HEADER_SIZE;

            if content.len() > max_len {
                self.flush(&self.buffer);
                self.recovery_window
                    .add(self.sequence_number, self.buffer.clone().into());
                self.sequence_number += 1;
                self.buffer.clear();
            }

            let mut header = (reliability.clone() as u8) << 5;
            if split {
                header |= FLAG_FRAGMENTED;
            }

            self.buffer.put_u8(header);
            self.buffer.put_u16((content.len() as u16) << 3);

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
                self.buffer.put_u8(0); // order index
            }

            if split {
                self.buffer.put_u32(split_count);
                self.buffer.put_u16(split_id);
                self.buffer.put_u32(split_index);
            }

            self.buffer.write_all(&content).unwrap();

            if reliability != Reliability::ReliableOrdered {
                self.flush(&self.buffer);
                self.recovery_window
                    .add(self.sequence_number, self.buffer.clone().into());
                self.sequence_number += 1;
                self.buffer.clear();
            }
        }

        self.msgbuf.clear();
    }

    /// Splits the encoded message into multiple fragments if it exceeds the maximum size of a datagram.
    /// It should return atleast one fragment.
    fn split<'a>(&self, bytes: &'a [u8]) -> Vec<&'a [u8]> {
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

    /// Decodes an ACK, NACK or a Datagram present in the provided buffer and handles it appropriately by
    /// responding etc.
    pub fn decode(
        &mut self,
        buffer: &[u8],
        ev: &mut EventWriter<RakNetEvent>,
        entity: Entity,
    ) -> Result<()> {
        let mut reader = Cursor::new(buffer);
        let header = reader.read_u8()?;

        if header == LOGIN_PACKET_ID {
            ev.send(RakNetEvent::DuplicateLogin(entity));
            return Ok(());
        }

        if header & FLAG_DATAGRAM == 0 {
            return Err(Error::new(
                ErrorKind::Other,
                "Buffer does not have a valid FLAG_DATAGRAM",
            ));
        }

        ev.send(RakNetEvent::LastActivity(entity, Instant::now()));

        if header & FLAG_ACK != 0 {
            return self.decode_ack(&mut reader, entity, ev);
        }

        if header & FLAG_NACK != 0 {
            return self.decode_nack(&mut reader, entity, ev);
        }

        self.decode_datagram(&mut reader, ev, entity)
    }

    /// This decodes a datagram from the provided buffer reader and returns any error whilst decoding it if any.
    /// If it contains a fragmented message, it tries to unsplit the message, it also handles fully processed packets.
    fn decode_datagram(
        &mut self,
        reader: &mut Cursor<&[u8]>,
        ev: &mut EventWriter<RakNetEvent>,
        entity: Entity,
    ) -> Result<()> {
        let seq = U24::<LE>::deserialize(reader)?.0;

        if !self.sequence_window.receive(seq) {
            return Ok(());
        }

        let mut count = 0;

        while reader.remaining() != 0 {
            let header = reader.read_u8()?;
            let split = (header & FLAG_FRAGMENTED) != 0;
            let reliability = Reliability::try_from((header & 224) >> 5)?;

            let mut length = U16::<BE>::deserialize(reader)?.0;
            length >>= 3;

            if length == 0 {
                return Err(Error::new(
                    ErrorKind::Other,
                    "RakNet Message content length cannot be 0",
                ));
            }

            let mut message_index = 0;

            if reliability.reliable() {
                message_index = U24::<LE>::deserialize(reader)?.0;
            }

            if reliability.sequenced() {
                reader.advance(3); // sequence index; this probably wouldn't happen for MCPE.
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

            reader.advance(length as usize);

            let content = &reader.get_ref()[start..end];

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
                    .split_window
                    .remove(&split_id)
                    .unwrap_or(SplitWindow::new(split_count));

                if splits.count != split_count {
                    return Err(Error::new(
                        ErrorKind::Other,
                        "Frame split count mismatch with the stored value for the given split ID.",
                    ));
                }

                if let Some(bytes) = splits.receive(split_index, content.to_vec()) {
                    self.handle_message(&bytes, ev, entity)?;
                    continue;
                }

                self.split_window.insert(split_id, splits);
            } else {
                self.handle_message(&content, ev, entity)?;
            }

            count += 1;

            if count > MAX_BATCHED_PACKETS {
                return Err(Error::new(
                    ErrorKind::Other,
                    "The datagram sent by the connection contains high number of batched messages",
                ));
            }
        }

        Ok(())
    }

    /// This decodes a Positive Acknowledgement Receipt from the other end of the connection by removing it
    /// from the recovery queue.
    fn decode_ack(
        &mut self,
        reader: &mut Cursor<&[u8]>,
        entity: Entity,
        ev: &mut EventWriter<RakNetEvent>,
    ) -> Result<()> {
        self.read_receipts(reader)?;
        trace!("[+] {:?} Received ACKs: {:?}", self.addr, self.receipts);

        while let Some(sequence) = self.receipts.pop_front() {
            self.recovery_window.acknowledge(sequence);
        }

        ev.send(RakNetEvent::Latency(entity, self.recovery_window.rtt()));
        Ok(())
    }

    /// This decodes a Negative Acknowledgement Receipt from the other end of the connection by retransmitting
    /// the packet from the recovery queue.
    fn decode_nack(
        &mut self,
        reader: &mut Cursor<&[u8]>,
        entity: Entity,
        ev: &mut EventWriter<RakNetEvent>,
    ) -> Result<()> {
        self.read_receipts(reader)?;
        trace!("[+] {:?} Received NACKs: {:?}", self.addr, self.receipts);

        while let Some(sequence) = self.receipts.pop_front() {
            if let Some(bytes) = self.recovery_window.retransmit(sequence) {
                self.flush(&bytes[..]);

                self.recovery_window.add(self.sequence_number, bytes);
                self.sequence_number += 1;
            }
        }

        ev.send(RakNetEvent::Latency(entity, self.recovery_window.rtt()));
        Ok(())
    }

    /// This function reads Receipts from the other end of the connection. These receipts may be an ACK
    /// or a NACK but this function does not need to know as it stores them in the same buffer.
    fn read_receipts(&mut self, reader: &mut Cursor<&[u8]>) -> Result<()> {
        let record_count = I16::<BE>::deserialize(reader)?.0;

        for _ in 0..record_count {
            let record_type = reader.read_u8()?;

            match record_type {
                0 => {
                    let start = U24::<LE>::deserialize(reader)?.0;
                    let end = U24::<LE>::deserialize(reader)?.0;

                    for seq in start..end {
                        self.receipts.push_back(seq);
                    }
                }
                1 => {
                    let seq = U24::<LE>::deserialize(reader)?.0;
                    self.receipts.push_back(seq);
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

    /// This flushes any receipts from our side such as ACK or NACK for the packets we received
    /// and we didn't receive respectively.
    pub fn flush_receipts(&mut self) {
        self.sequence_window.shift();

        if self.sequence_window.acks.len() > 0 {
            self.write_ack();
        }

        if self.sequence_window.nacks.len() > 0 {
            self.write_nack();
        }
    }

    /// Writes a Positive Acknowledgement Receipt to the other end of the connection containing all the
    /// sequence numbers that we received.
    fn write_ack(&mut self) {
        trace!(
            "[-] {:?} Sending ACKs {:?}",
            self.addr,
            &self.sequence_window.acks
        );
        let _ = self.receiptbuf.write_u8(FLAG_DATAGRAM | FLAG_ACK);
        self.write_receipts(false);
    }

    /// Writes a Negative Acknowledgement Receipt to the other end of the connection containing all the
    /// sequence numbers that we did not receive.
    fn write_nack(&mut self) {
        trace!(
            "[-] {:?} Sending NACKs {:?}",
            self.addr,
            &self.sequence_window.nacks
        );
        let _ = self.receiptbuf.write_u8(FLAG_DATAGRAM | FLAG_NACK);
        self.write_receipts(true);
    }

    /// This function contains all the logic for serializing a Receipt packet in RakNet. It immediately flushes
    /// the encoded
    fn write_receipts(&mut self, nack: bool) {
        let sequences = if nack {
            &mut self.sequence_window.nacks
        } else {
            &mut self.sequence_window.acks
        };

        sequences.sort();
        self.receiptbuf.put_i16(0);

        let mut first = sequences[0];
        let mut last = sequences[0];
        let mut record_count = 0;

        for index in 0..sequences.len() {
            let sequence = sequences[index];

            if sequence == last + 1 {
                last = sequence;

                if index != sequences.len() - 1 {
                    continue;
                }
            }

            if first == last {
                self.receiptbuf.put_u8(1);
                U24::<LE>::new(first).serialize(&mut self.receiptbuf);
            } else {
                self.receiptbuf.put_u8(0);
                U24::<LE>::new(first).serialize(&mut self.receiptbuf);
                U24::<LE>::new(last).serialize(&mut self.receiptbuf);
            }

            first = sequence;
            last = sequence;
            record_count += 1;
        }

        let mut reserved = &mut self.receiptbuf[1..3];
        reserved.put_i16(record_count);

        self.socket.send_to(&self.receiptbuf, self.addr).unwrap();
        self.receiptbuf.clear();
        sequences.clear();
    }

    /// Decodes a RakNet Message from the provided buffer and flushes it's response if required
    /// (for mostly Internal Packets) immediately.
    fn handle_message(
        &mut self,
        buffer: &[u8],
        ev: &mut EventWriter<RakNetEvent>,
        entity: Entity,
    ) -> Result<()> {
        let mut reader = Cursor::new(buffer);
        let message = Message::deserialize(&mut reader)?;

        trace!("[+] {:?} {:?}", self.addr, message);

        match message {
            Message::ConnectedPing { client_timestamp } => {
                let resp = Message::ConnectedPong {
                    client_timestamp: client_timestamp.clone(),
                    server_timestamp: client_timestamp,
                };

                self.encode(resp, Reliability::Unreliable);
            }
            Message::ConnectedPong {
                client_timestamp,
                server_timestamp,
            } => {
                let ping = server_timestamp.0 - client_timestamp.0;
                ev.send(RakNetEvent::Ping(entity, ping as u64));
            }
            Message::ConnectionRequest {
                client_guid: _,
                request_timestamp,
                secure: _,
            } => {
                let resp = Message::ConnectionRequestAccepted {
                    client_address: UDPAddress(self.addr),
                    system_index: I16::new(0),
                    system_addresses: SystemAddresses,
                    request_timestamp: request_timestamp.clone(),
                    accept_timestamp: request_timestamp,
                };

                self.encode(resp, Reliability::Unreliable);
            }
            Message::ConnectionRequestAccepted {
                client_address: _,
                system_index: _,
                system_addresses,
                request_timestamp,
                accept_timestamp,
            } => {
                let resp = Message::NewIncomingConnection {
                    server_address: UDPAddress(self.addr),
                    system_addresses,
                    request_timestamp,
                    accept_timestamp,
                };

                self.encode(resp, Reliability::Unreliable);
                ev.send(RakNetEvent::ConnectionEstablished(self.addr, entity));
            }
            Message::NewIncomingConnection {
                server_address: _,
                system_addresses: _,
                request_timestamp: _,
                accept_timestamp: _,
            } => {
                ev.send(RakNetEvent::ConnectionEstablished(self.addr, entity));
            }
            Message::GamePacket { data } => {
                ev.send(RakNetEvent::IncomingBatch(entity, data.to_vec()));
                info!("{:?} {:?}", self.addr, data);
            }
            Message::DisconnectNotification {} => {
                ev.send(RakNetEvent::Disconnect(entity));
            }
            Message::DetectLostConnections {} => {
                let resp = Message::ConnectedPing {
                    client_timestamp: I64::new(unix_timestamp() as i64),
                };

                self.encode(resp, Reliability::Unreliable);
            }
            Message::IncompatibleProtocolVersion {
                server_protocol,
                magic: _,
                server_guid: _,
            } => ev.send(RakNetEvent::IncompatibleProtocol(entity, server_protocol.0)),
            _ => {}
        }

        Ok(())
    }

    /// Tries to flush the packets written so far to the other end of the connection if the buffer
    /// is not empty.
    pub fn try_flush(&mut self) {
        if self.buffer.len() == 0 {
            return;
        }

        self.flush(&self.buffer);
        self.sequence_number += 1;
        self.buffer.clear();
    }

    /// Flushes the provided encoded datagram message by appending the header of the datagram with
    /// a new sequence number and flushes it immediately to the socket connection.
    fn flush(&self, buffer: &[u8]) {
        let mut header = [0u8; 4];
        let mut writer = header.as_mut_slice();

        writer.put_u8(FLAG_DATAGRAM | FLAG_NEEDS_B_AND_AS);
        U24::<LE>::new(self.sequence_number).serialize(&mut writer);

        let buffer: &[u8] = &[&header[..], &buffer[..]].concat();
        self.socket.send_to(&buffer, self.addr).unwrap();
    }

    /// Handles graceful disconnection of the client, it flushes all the remaining packets we have written so far
    /// and also sends the Disconnect Notification to the client.
    pub fn disconnect(&mut self) {
        self.encode(
            Message::DisconnectNotification {},
            Reliability::ReliableOrdered,
        );
        self.try_flush();
    }
}
