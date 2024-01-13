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
    C2SGamePacket(Entity, Vec<u8>),
    S2CGamePacket(Entity, Vec<u8>),
}

#[derive(Event)]
pub enum NetworkEvent {
    ConnectionRequest(Entity),
    ConnectionEstablished(Entity),
    C2SPacket(Entity, Bytes),
    S2CPacket(Entity, Bytes),
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
