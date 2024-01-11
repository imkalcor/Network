use bevy::ecs::entity::Entity;
use bevy::ecs::event::EventWriter;
use bevy::ecs::system::{Query, Resource};
use binary::Binary;
use bytes::BytesMut;
use log::{debug, trace};

use std::collections::HashMap;
use std::io::{Cursor, Result};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Instant;

use crate::generic::events::{BlockReason, RakNetEvent};
use crate::generic::timestamp;
use crate::protocol::message::Message;
use crate::protocol::{MAX_INVALID_MSGS, MAX_MSGS_PER_SEC, MAX_MTU_SIZE, RAKNET_BLOCK_DUR};

use super::conn::RakNetDecoder;

/// Minecraft Listener built on top of the UDP Protocol with built-in reliability (also known as RakNet)
#[derive(Resource)]
pub struct Listener {
    socket: Arc<UdpSocket>,
    addr: SocketAddr,

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

    /// Returns the local address that the Listener is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Tries to receive a datagram from the UdpSocket, returns if the operation is successful
    pub fn try_recv(&mut self) -> Result<(usize, SocketAddr)> {
        self.socket.recv_from(&mut self.read_buf)
    }

    /// Check if the sender is blocked or not. Unblocks the sender if the block duration has been achieved.
    /// Returns true if the sender is still blocked.
    pub fn is_blocked(&mut self, addr: SocketAddr) -> bool {
        if let Some(expiry) = self.blocked.get(&addr) {
            if expiry > &timestamp() {
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
            .insert(addr, timestamp() + RAKNET_BLOCK_DUR.as_secs());
        ev.send(RakNetEvent::Blocked(addr, RAKNET_BLOCK_DUR, reason));
    }

    /// Tries to parse a connected message from the Listener's internal read buffer. Returns whether the
    /// operation failed due to possible corruption in the message.
    pub fn try_parse_connected_message(
        &self,
        addr: SocketAddr,
        query: &mut Query<&mut RakNetDecoder>,
    ) -> bool {
        if let Some(entity) = self.connections.get(&addr) {
            let mut decoder = query.get_mut(*entity).unwrap();
            return decoder.decode(&self.read_buf);
        }

        true
    }

    /// Tries to parse an Unconnected Message from the Listener's internal read buffer. Returns
    /// whether the operation failed due to possible corruption in the message.
    pub fn try_parse_unconnected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        ev: &mut EventWriter<RakNetEvent>,
    ) -> bool {
        let mut reader = Cursor::new(&self.read_buf[..len]);
        let message = match Message::deserialize(&mut reader) {
            Ok(msg) => msg,
            Err(e) => {
                debug!("[Network Error]: {}", e.to_string());
                return false;
            }
        };

        trace!("[+] {:?} {:?}", addr, message);

        match message {
            Message::UnconnectedPing {
                send_timestamp,
                magic,
                client_guid,
            } => {
                ev.send(RakNetEvent::Ping(addr));
            }
            _ => {}
        }

        true
    }
}
