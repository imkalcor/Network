use bevy::ecs::bundle::Bundle;
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::EventWriter;
use bevy::ecs::system::{Commands, Query};
use bevy::ecs::world::World;
use binary::datatypes::{Bool, I64, U16, U8};
use binary::prefixed::{Str, UnsizedBytes};
use binary::Binary;
use bytes::BytesMut;
use commons::utils::unix_timestamp;
use log::{debug, info, trace};

use crate::generic::events::RakNetEvent;
use crate::net::stream::{RakStream, StreamBundle};
use crate::protocol::binary::{Magic, UDPAddress};
use crate::protocol::mcpe::{
    BroadcastGamemode, MaxPlayers, MinecraftProtocol, MinecraftVersion, OnlinePlayers, PrimaryMotd,
    SecondaryMotd,
};
use crate::protocol::message::Message;
use crate::protocol::{
    CLIENT_PADDING_DECREASE, MAX_INVALID_MSGS, MAX_MSGS_PER_SEC, MAX_MTU_SIZE, PROTOCOL_VERSION,
    RAKNET_BLOCK_DUR, UDP_HEADER_SIZE,
};
use std::collections::HashMap;
use std::io::{Cursor, Error, ErrorKind, Result};
use std::net::{SocketAddr, UdpSocket};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::stream::{NetworkInfo, NetworkStatus};

/// Mappings contains all the useful maps that store data such as the connections <-> Entity map, and various other maps
/// that help in preventing packet spamming, corrupt packets, etc.
#[derive(Component, Default)]
pub struct Mappings {
    connections: HashMap<SocketAddr, Entity>,
    blocked: HashMap<SocketAddr, u64>,
    packets_per_sec: HashMap<SocketAddr, (Instant, u8)>,
    invalid_packets: HashMap<SocketAddr, u8>,
}

/// ServerBundle is the bundle used to spawn a RakNet server. A RakNet server has multiple extra components from a client such
/// as various components used for building the unconnected pong message.
#[derive(Bundle)]
pub struct ServerBundle {
    pub socket: RakSocket,
    pub info: SocketInfo,
    pub mappings: Mappings,
    pub primary_motd: PrimaryMotd,
    pub secondary_motd: SecondaryMotd,
    pub online_players: OnlinePlayers,
    pub max_players: MaxPlayers,
    pub gamemode: BroadcastGamemode,
    pub protocol: MinecraftProtocol,
    pub version: MinecraftVersion,
}

impl ServerBundle {
    pub fn new(addr: &str) -> Self {
        let socket = RakSocket::new(addr, true).unwrap();
        let addr = socket.udp.local_addr().unwrap();
        let guid = rand::random();

        Self {
            socket,
            info: SocketInfo { addr, guid },
            mappings: Mappings::default(),
            primary_motd: PrimaryMotd::new("RakNet"),
            secondary_motd: SecondaryMotd::new("blazingly fast!"),
            online_players: OnlinePlayers::new(0),
            max_players: MaxPlayers::new(1000),
            gamemode: BroadcastGamemode::new("Survival"),
            protocol: MinecraftProtocol::new(600),
            version: MinecraftVersion::new("1.20.51"),
        }
    }
}

/// ClientBundle is the bundle used to spawn a RakNet client. It contains additional components from a RakNet server such as the
/// stream components, stream info because RakNet client is an established connection.
#[derive(Bundle)]
pub struct ClientBundle {
    pub socket: RakSocket,
    pub info: SocketInfo,
    pub stream: StreamBundle,
}

/// SocketInfo contains information about a RakSocket such as the address it's bound to, it's guid.
#[derive(Component)]
pub struct SocketInfo {
    pub addr: SocketAddr,
    pub guid: i64,
}

/// RakSocket is built on top of the UdpSocket and handles the reading and writing of unconnected messages from/to the other end of the
/// connection. It handles the login sequence of clients (logging into a server) and server (for clients logging into it).
#[derive(Component)]
pub struct RakSocket {
    pub udp: Arc<UdpSocket>,
    pub read_buf: BytesMut,
    pub write_buf: BytesMut,
}

impl RakSocket {
    /// Creates and returns a new instance of Listener.
    pub fn new(addr: &str, non_blocking: bool) -> Result<Self> {
        match UdpSocket::bind(addr) {
            Ok(socket) => {
                socket.set_nonblocking(non_blocking).unwrap();

                Ok(Self {
                    udp: socket.into(),
                    read_buf: BytesMut::zeroed(MAX_MTU_SIZE),
                    write_buf: BytesMut::with_capacity(MAX_MTU_SIZE),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Connects to the specified address running a RakNet server. If successful, it spawns an entity from the StreamBundle
    /// and returns it'd ID.
    pub fn connect(addr: &str, world: &mut World) -> Result<Entity> {
        // Creates a new RakSocket and binds it on any random port with blocking mode.
        let mut socket = RakSocket::new("127.0.0.1:0", false)?;
        let local_addr = socket.udp.local_addr().unwrap();
        let remote_addr: SocketAddr = SocketAddr::from_str(addr).unwrap();

        // Configure the socket to have a read delay of 1 second so it could be useful when discovering the MTU size of
        // the connection later and in general is helpful.
        socket.udp.connect(remote_addr)?;
        socket
            .udp
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();

        // We try to send a Unconnected Ping message to the other end of the connection to get it's status, MOTD, and to check if it's alive.
        let guid = rand::random();
        let msg = Message::UnconnectedPing {
            send_timestamp: I64::new(unix_timestamp() as i64),
            magic: Magic,
            client_guid: I64::new(guid),
        };

        socket.write(msg)?;

        // Wait for an UnconnectedPong message from the other end, return if no message is received
        match socket.read()? {
            Message::UnconnectedPong {
                send_timestamp: _,
                server_guid: _,
                magic: _,
                data,
            } => {
                debug!("Connecting to {:?}", data);
            }
            _ => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Expected UnconnectedPong message from the other end of the connection",
                ))
            }
        }

        // We try to discuss the MTU size of the other end of the connection. In order to do that, we send an
        // empty buffer of size equivalent to the MAX_MTU_SIZE - 46 (28 UDP Overhead, 1 packet ID, 16 magic, 1 protocol version).
        // This padding is decreased every second by cpnfigured rate to be able to discover the maximum size of datagram the server can handle.
        let mut mtu_size = MAX_MTU_SIZE;

        loop {
            let size = mtu_size - UDP_HEADER_SIZE - 16 - 1 - 1;
            let emptybytes = BytesMut::zeroed(size);

            let msg = Message::OpenConnectionRequest1 {
                magic: Magic,
                protocol: U8::new(PROTOCOL_VERSION),
                emptybuf: UnsizedBytes::new(&emptybytes),
            };

            socket.write(msg)?;

            if let Ok(msg) = socket.read() {
                match msg {
                    Message::OpenConnectionReply1 {
                        magic,
                        server_guid: _,
                        secure: _,
                        server_mtu,
                    } => {
                        mtu_size = server_mtu.0 as usize;

                        // Write the OpenConnectionRequest2 message to the other end of the connection.
                        let msg = Message::OpenConnectionRequest2 {
                            magic,
                            server_address: UDPAddress(remote_addr),
                            client_mtu: server_mtu,
                            client_guid: I64::new(guid),
                        };
                        socket.write(msg)?;

                        break;
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            "Expected OpenConnectionReply1 from the other end of the connection",
                        ))
                    }
                }
            };

            mtu_size -= CLIENT_PADDING_DECREASE;
        }

        // Expect a OpenConnectionReply2 message from the other end of the connection.
        match socket.read()? {
            Message::OpenConnectionReply2 {
                magic: _,
                server_guid: _,
                client_address: _,
                mtu_size: _,
                secure: _,
            } => {}
            _ => {
                return Err(Error::new(
                    ErrorKind::Other,
                    "Expected OpenConnectionReply2 message from the other end of the connection",
                ))
            }
        }

        let udp = socket.udp.clone();
        let id = world
            .spawn(ClientBundle {
                socket,
                info: SocketInfo {
                    addr: local_addr,
                    guid,
                },
                stream: StreamBundle {
                    info: NetworkInfo {
                        local_addr,
                        remote_addr,
                    },
                    status: NetworkStatus {
                        ping: 0,
                        latency: Duration::from_secs(0),
                        last_activity: Instant::now(),
                    },
                    rakstream: RakStream::new(remote_addr, udp, mtu_size),
                },
            })
            .id();

        Ok(id)
    }

    /// Check if the sender is blocked or not. Unblocks the sender if the block duration has been achieved.
    /// Returns true if the sender is still blocked.
    pub fn is_blocked(&mut self, addr: SocketAddr, mappings: &mut Mappings) -> bool {
        if let Some(expiry) = mappings.blocked.get(&addr) {
            if expiry > &unix_timestamp() {
                return true;
            }

            mappings.blocked.remove(&addr);
        }

        return false;
    }

    /// Checks if the sender does not exceed the maximum number of packets per second. Returns true
    /// if the number of packets exceed the allowed.
    pub fn check_packet_spam(&mut self, addr: SocketAddr, mappings: &mut Mappings) -> bool {
        let (mut instant, mut packets) = mappings
            .packets_per_sec
            .remove(&addr)
            .unwrap_or((Instant::now(), 0));

        if instant.elapsed().as_millis() < 1000 {
            packets += 1;

            if packets == MAX_MSGS_PER_SEC {
                self.block(addr, mappings);
                return true;
            }
        } else {
            instant = Instant::now();
            packets = 0;
        }

        mappings.packets_per_sec.insert(addr, (instant, packets));
        return false;
    }

    /// Checks if the sender exceeds the maximum number of invalid packets. Blocks the sender if it exceeds
    /// the allowed limit.
    pub fn check_invalid_packets(&mut self, addr: SocketAddr, mappings: &mut Mappings) {
        let invalid_packets = mappings.invalid_packets.get(&addr).unwrap_or(&0) + 1;

        if invalid_packets == MAX_INVALID_MSGS {
            mappings.invalid_packets.remove(&addr);
            self.block(addr, mappings);
            return;
        }

        mappings.invalid_packets.insert(addr, invalid_packets);
    }

    /// Blocks a provided IP address for the specified reason and writes an event to the Bevy Runtime.
    pub fn block(&mut self, addr: SocketAddr, mappings: &mut Mappings) {
        mappings
            .blocked
            .insert(addr, unix_timestamp() + RAKNET_BLOCK_DUR.as_secs());
    }

    /// Checks if the message received on the buffer is a Connected Message. Returns whether the message was a connected
    /// one and that it handled it appropriately.
    pub fn handle_connected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        query: &mut Query<&mut RakStream>,
        ev: &mut EventWriter<RakNetEvent>,
        mappings: &mut Mappings,
    ) -> bool {
        if let Some(entity) = mappings.connections.get(&addr) {
            if let Ok(mut stream) = query.get_mut(*entity) {
                if let Err(e) = stream.decode(&self.read_buf[..len], ev, *entity) {
                    debug!("[Network Error] {}", e.to_string());

                    ev.send(RakNetEvent::MalformedPackets(*entity));
                }

                return true;
            }

            // Remove the entry because the entity did not exist.
            mappings.connections.remove(&addr);
            return true;
        }

        false
    }

    /// Handles an unconnected message received on the buffer.
    pub fn handle_unconnected_message(
        &mut self,
        addr: SocketAddr,
        len: usize,
        status: &str,
        commands: &mut Commands,
        ev: &mut EventWriter<RakNetEvent>,
        info: &SocketInfo,
        mappings: &mut Mappings,
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
                    server_guid: I64::new(info.guid),
                    magic,
                    data: Str::new(status),
                };

                self.write_to(addr, resp)?;
            }
            Message::UnconnectedPingOpenConnections {
                send_timestamp,
                magic,
                client_guid: _,
            } => {
                let resp = Message::UnconnectedPong {
                    send_timestamp,
                    server_guid: I64::new(info.guid),
                    magic,
                    data: Str::new(status),
                };

                self.write_to(addr, resp)?;
            }
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
                        server_guid: I64::new(info.guid),
                    };

                    self.write_to(addr, resp)?;
                    return Ok(());
                }

                let resp = Message::OpenConnectionReply1 {
                    magic,
                    server_guid: I64::new(info.guid),
                    secure: Bool::new(false),
                    server_mtu: U16::new(server_mtu as u16),
                };

                self.write_to(addr, resp)?;
                ev.send(RakNetEvent::ConnectionRequest(addr));
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
                    server_guid: I64::new(info.guid),
                    client_address: UDPAddress(addr),
                    mtu_size: U16::new(mtu_size as u16),
                    secure: Bool::new(false),
                };

                self.write_to(addr, resp)?;

                let entity = commands.spawn(StreamBundle {
                    info: NetworkInfo {
                        local_addr: server_address.0,
                        remote_addr: addr,
                    },
                    status: NetworkStatus {
                        ping: 0,
                        latency: Duration::from_secs(0),
                        last_activity: Instant::now(),
                    },
                    rakstream: RakStream::new(addr, self.udp.clone(), mtu_size),
                });

                mappings.connections.insert(addr, entity.id());
                info!("Spawned Entity: {:?}", entity.id().index());
            }
            _ => {}
        }

        Ok(())
    }

    /// Reads an unconnected message from the connected stream.
    fn read(&mut self) -> Result<Message> {
        let len = self.udp.recv(&mut self.read_buf)?;
        let mut reader = Cursor::new(&self.read_buf[..len]);
        Message::deserialize(&mut reader)
    }

    /// Writes an unconnected message to the connected stream.
    fn write(&mut self, message: Message) -> Result<()> {
        message.serialize(&mut self.write_buf);
        self.udp.send(&self.write_buf)?;
        self.write_buf.clear();

        Ok(())
    }

    /// Writes an unconnected message to the provided address and flushes it immediately.
    fn write_to(&mut self, addr: SocketAddr, message: Message) -> Result<()> {
        message.serialize(&mut self.write_buf);
        self.udp.send_to(&self.write_buf, addr)?;
        self.write_buf.clear();

        Ok(())
    }
}
