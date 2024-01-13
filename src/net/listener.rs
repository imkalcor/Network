use bevy::ecs::entity::Entity;
use bevy::ecs::event::EventWriter;
use bevy::ecs::system::{Commands, Query, Resource};
use binary::datatypes::{Bool, I64, U16, U8};
use binary::prefixed::Str;
use binary::Binary;
use bytes::BytesMut;
use commons::utils::unix_timestamp;
use log::trace;

use crate::generic::events::{BlockReason, RakNetEvent};
use crate::net::conn::{NetworkBundle, RakStream};
use crate::protocol::binary::UDPAddress;
use crate::protocol::message::Message;
use crate::protocol::{
    MAX_INVALID_MSGS, MAX_MSGS_PER_SEC, MAX_MTU_SIZE, PROTOCOL_VERSION, RAKNET_BLOCK_DUR,
    UDP_HEADER_SIZE,
};
use rand::Rng;
use std::collections::HashMap;
use std::io::{Cursor, Result};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::conn::NetworkInfo;

/// Minecraft Listener built on top of the UDP Protocol with built-in reliability (also known as RakNet)
#[derive(Resource)]
pub struct Listener {
    pub addr: SocketAddr,
    pub guid: i64,

    socket: Arc<UdpSocket>,

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
                    addr,
                    guid: rand::thread_rng().gen(),
                    socket: socket.into(),
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
            packets = 0;
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
    pub fn handle_connected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        query: &mut Query<(&mut RakStream, &mut NetworkInfo)>,
        ev: &mut EventWriter<RakNetEvent>,
    ) -> Result<bool> {
        if let Some(entity) = self.connections.get(&addr) {
            let (mut conn, mut info) = query.get_mut(*entity).unwrap();
            conn.decode(&self.read_buf[..len], &mut info, ev, *entity)?;

            return Ok(true);
        }

        return Ok(false);
    }

    /// Handles an unconnected message received on the buffer.
    pub fn handle_unconnected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        commands: &mut Commands,
    ) -> Result<()> {
        let mut reader = Cursor::new(&self.read_buf[..len]);
        let message = Message::deserialize(&mut reader)?;

        trace!("[+] {:?} {:?}", addr, message);

        match message {
            Message::UnconnectedPing {
                send_timestamp,
                magic,
                client_guid: _,
            } => {
                let resp = Message::UnconnectedPong {
                    send_timestamp,
                    server_guid: I64::new(self.guid),
                    magic,
                    data: Str::new("MCPE;Dedicated Server;390;1.14.60;0;10;13253860892328930865;Bedrock level;Survival;1;19132;19133;"),
                };

                self.write_message(addr, resp)?;
            }
            Message::UnconnectedPingOpenConnections {
                send_timestamp,
                magic,
                client_guid: _,
            } => {
                let resp = Message::UnconnectedPong {
                    send_timestamp,
                    server_guid: I64::new(self.guid),
                    magic,
                    data: Str::new("MCPE;Dedicated Server;390;1.14.60;0;10;13253860892328930865;Bedrock level;Survival;1;19132;19133;"),
                };

                self.write_message(addr, resp)?;
            }
            Message::UnconnectedPong {
                send_timestamp: _,
                server_guid: _,
                magic: _,
                data: _,
            } => {}
            Message::OpenConnectionRequest1 {
                magic,
                protocol,
                emptybuf: _,
            } => {
                let mut server_mtu = reader.get_ref().len() + UDP_HEADER_SIZE;
                if server_mtu > MAX_MTU_SIZE {
                    server_mtu = MAX_MTU_SIZE;
                }

                if protocol.0 != PROTOCOL_VERSION {
                    let resp = Message::IncompatibleProtocolVersion {
                        server_protocol: U8::new(PROTOCOL_VERSION),
                        magic,
                        server_guid: I64::new(self.guid),
                    };

                    self.write_message(addr, resp)?;
                    return Ok(());
                }

                let resp = Message::OpenConnectionReply1 {
                    magic,
                    server_guid: I64::new(self.guid),
                    secure: Bool::new(false),
                    server_mtu: U16::new(server_mtu as u16),
                };

                self.write_message(addr, resp)?;
            }
            Message::OpenConnectionRequest2 {
                magic,
                server_address,
                client_mtu,
                client_guid: _,
            } => {
                let mut mtu_size = client_mtu.0 as usize;
                if mtu_size > MAX_MTU_SIZE {
                    mtu_size = MAX_MTU_SIZE
                }

                let resp = Message::OpenConnectionReply2 {
                    magic,
                    server_guid: I64::new(self.guid),
                    client_address: UDPAddress(addr),
                    mtu_size: U16::new(mtu_size as u16),
                    secure: Bool::new(false),
                };

                self.write_message(addr, resp)?;

                let entity = commands.spawn(NetworkBundle {
                    info: NetworkInfo {
                        last_activity: Instant::now(),
                        latency: Duration::from_secs(0),
                        ping: 0,
                        local_addr: server_address.0,
                        remote_addr: addr,
                    },
                    rakstream: RakStream::new(addr, self.socket.clone(), mtu_size),
                });

                self.connections.insert(addr, entity.id());
            }
            _ => {}
        }

        Ok(())
    }

    /// Writes an unconnected message to the provided address and flushes it immediately.
    fn write_message(&mut self, addr: SocketAddr, message: Message) -> Result<()> {
        message.serialize(&mut self.write_buf);
        self.socket.send_to(&self.write_buf, addr)?;
        self.write_buf.clear();

        Ok(())
    }
}
