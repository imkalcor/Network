use std::{net::SocketAddr, time::Duration};

use bevy::ecs::{entity::Entity, event::Event};
use bytes::Bytes;

/// RakNetEvent contains various variants that are useful in debugging various
/// RakNet connection stages and to receive and send a RakNet Game Packet batch.
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

/// NetworkEvent can be used for handling various Minecraft related Login Process events
/// and to receive and send a Minecraft (Optionally Compressed & Encrypted) Packet Batch.
#[derive(Event)]
pub enum NetworkEvent {
    ConnectionRequest(Entity),
    ConnectionEstablished(Entity),
    C2SPacket(Entity, Bytes),
    S2CPacket(Entity, Bytes),
}

/// RakNet Disconnect Reason sent or received in the RakNet Disconnect packet.
#[derive(Debug)]
pub enum DisconnectReason {
    IncompatibleProtocol,
    ClientDisconnect,
    ServerDisconnect,
    ClientTimeout,
    ServerShutdown,
    DuplicateLogin,
}

/// BlockReason is enum variants of what kind of block an IP Address is facing.
#[derive(Debug)]
pub enum BlockReason {
    PacketSpam,
    MalformedPackets,
}
