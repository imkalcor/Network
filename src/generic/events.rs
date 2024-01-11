use std::{net::SocketAddr, time::Duration};

use bevy::ecs::{entity::Entity, event::Event};
use bytes::Bytes;

#[derive(Event)]
pub enum RakNetEvent {
    Ping(SocketAddr),
    Blocked(SocketAddr, Duration, BlockReason),
    ConnectionRequest(SocketAddr),
    ConnectionEstablished(SocketAddr, Entity),
    Disconnect(Entity, DisconnectReason),
    IncomingBatch(Entity, Bytes),
    OutgoingBatch(Entity, Bytes),
}

#[derive(Event)]
pub enum NetworkEvent {
    WritePacket(Entity),
    ReadPacket(Entity),
}

#[derive(Debug)]
pub enum DisconnectReason {
    IncompatibleProtocol,
    ClientDisconnect,
    ServerDisconnect,
    ClientTimeout,
    ServerShutdown,
    DuplicateLogin,
    Custom(String),
}

#[derive(Debug)]
pub enum BlockReason {
    PacketSpam,
    MalformedPackets,
}
