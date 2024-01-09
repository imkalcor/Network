use std::{net::SocketAddr, time::Duration};

use bevy::ecs::{entity::Entity, event::Event};
use bytes::Bytes;

#[derive(Event)]
pub enum RakNetEvent {
    Ping(SocketAddr),
    Blocked(SocketAddr, Duration, BlockReason),
    Connecting(SocketAddr),
    Connected(SocketAddr, Entity),
    GamePacket(Entity, SocketAddr, Bytes),
    Disconnect(Entity, SocketAddr, DisconnectReason),
}

#[derive(Debug)]
pub enum DisconnectReason {
    IncompatibleProtocol,
    ClientDisconnect,
    ServerDisconnect,
    ClientTimeout,
    ServerShutdown,
    DuplicateLogin,
}

#[derive(Debug)]
pub enum BlockReason {
    PacketSpam,
    MalformedPackets,
}
