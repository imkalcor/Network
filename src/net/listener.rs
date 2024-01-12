use bevy::ecs::entity::Entity;
use bevy::ecs::event::EventWriter;
use bevy::ecs::system::{Query, Resource};
use binary::Binary;
use bytes::BytesMut;
use commons::utils::unix_timestamp;
use log::debug;

use std::collections::HashMap;
use std::io::{Cursor, Error, ErrorKind, Result};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Instant;

use crate::generic::events::{BlockReason, RakNetEvent};
use crate::protocol::message::Message;
use crate::protocol::{MAX_INVALID_MSGS, MAX_MSGS_PER_SEC, MAX_MTU_SIZE, RAKNET_BLOCK_DUR};

use super::conn::{NetworkInfo, RakNetDecoder};

/// Minecraft Listener built on top of the UDP Protocol with built-in reliability (also known as RakNet)
#[derive(Resource)]
pub struct Listener {
    socket: Arc<UdpSocket>,
    pub addr: SocketAddr,

    read_buf: BytesMut,
    write_buf: BytesMut,

    connections: HashMap<SocketAddr, Entity>,
    blocked: HashMap<SocketAddr, u64>,

    packets_per_sec: HashMap<SocketAddr, (Instant, u8)>,
    invalid_packets: HashMap<SocketAddr, u8>,
}

impl Listener {
    /// Creates and returns a new instance of Listener.
    pub fn new(addr: SocketAddr) -> Result<Self> {
        match UdpSocket::bind(addr) {
            Ok(socket) => {
                socket.set_nonblocking(true).unwrap();

                Ok(Self {
                    socket: socket.into(),
                    addr,
                    read_buf: BytesMut::zeroed(MAX_MTU_SIZE),
                    write_buf: BytesMut::with_capacity(MAX_MTU_SIZE),
                    connections: HashMap::new(),
                    blocked: HashMap::new(),
                    packets_per_sec: HashMap::new(),
                    invalid_packets: HashMap::new(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Tries to receive a datagram from the UdpSocket, returns if the operation is successful
    pub fn try_recv(&mut self) -> Result<(usize, SocketAddr)> {
        self.socket.recv_from(&mut self.read_buf)
    }

    /// Check if the sender is blocked or not. Unblocks the sender if the block duration has been achieved.
    /// Returns true if the sender is still blocked.
    pub fn is_blocked(&mut self, addr: SocketAddr) -> bool {
        if let Some(expiry) = self.blocked.get(&addr) {
            if expiry > &unix_timestamp() {
                return true;
            }

            self.blocked.remove(&addr);
        }

        return false;
    }

    /// Checks if the sender does not exceed the maximum number of packets per second. Returns true
    /// if the number of packets exceed the allowed.
    pub fn check_packet_spam(
        &mut self,
        addr: SocketAddr,
        ev: &mut EventWriter<RakNetEvent>,
    ) -> bool {
        let (mut instant, mut packets) = self
            .packets_per_sec
            .remove(&addr)
            .unwrap_or((Instant::now(), 0));

        if instant.elapsed().as_millis() < 1000 {
            packets += 1;

            if packets == MAX_MSGS_PER_SEC {
                self.block(addr, ev, BlockReason::PacketSpam);
                return true;
            }
        } else {
            instant = Instant::now();
        }

        self.packets_per_sec.insert(addr, (instant, packets));
        return false;
    }

    /// Checks if the sender exceeds the maximum number of invalid packets. Blocks the sender if it exceeds
    /// the allowed limit.
    pub fn check_invalid_packets(&mut self, addr: SocketAddr, ev: &mut EventWriter<RakNetEvent>) {
        let invalid_packets = self.invalid_packets.get(&addr).unwrap_or(&0) + 1;

        if invalid_packets == MAX_INVALID_MSGS {
            self.invalid_packets.remove(&addr);
            self.block(addr, ev, BlockReason::MalformedPackets);
            return;
        }

        self.invalid_packets.insert(addr, invalid_packets);
    }

    /// Blocks a provided IP address for the specified reason and writes an event to the Bevy Runtime.
    pub fn block(
        &mut self,
        addr: SocketAddr,
        ev: &mut EventWriter<RakNetEvent>,
        reason: BlockReason,
    ) {
        self.blocked
            .insert(addr, unix_timestamp() + RAKNET_BLOCK_DUR.as_secs());
        ev.send(RakNetEvent::Blocked(addr, RAKNET_BLOCK_DUR, reason));
    }

    /// Checks if the message received on the buffer is a Connected Message. If it is, then it processes the message
    /// and handles it gracefully.
    pub fn try_handle_connected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        query: &mut Query<(&mut RakNetDecoder, &mut NetworkInfo)>,
    ) -> Result<()> {
        if let Some(entity) = self.connections.get(&addr) {
            let (mut decoder, mut info) = query.get_mut(*entity).unwrap();
            let messages = decoder.decode(&self.read_buf[..len], &mut info)?;

            if let Some(messages) = messages {
                for message in messages {
                    let mut reader = Cursor::new(&message[..]);
                    let message = Message::deserialize(&mut reader)?;

                    self.handle_connected_message(addr, message)?;
                }
            }
        }

        return Err(Error::new(
            ErrorKind::Other,
            "Entity for the address does not exist",
        ));
    }

    /// Handles an unconnected message received on the buffer.
    pub fn handle_unconnected_message(&mut self, addr: SocketAddr, len: usize) -> Result<()> {
        let mut reader = Cursor::new(&self.read_buf[..len]);
        let message = Message::deserialize(&mut reader)?;

        debug!("[+] {:?} {:?}", addr, message);

        Ok(())
    }

    /// Handles a connected message received on the buffer.
    fn handle_connected_message(&mut self, addr: SocketAddr, message: Message) -> Result<()> {
        debug!("[+] {:?} {:?}", addr, message);
        Ok(())
    }
}
