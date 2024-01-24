use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use bevy::ecs::{entity::Entity, event::Event};
use bytes::Bytes;

/// RakNetEvent contains various variants that are useful in debugging various
/// RakNet connection stages and to receive and send a RakNet Game Packet batch.
#[derive(Event)]
pub enum RakNetEvent {
    ConnectionRequest(SocketAddr),
    ConnectionEstablished(SocketAddr, Entity),
    MalformedPackets(Entity),
    DuplicateLogin(Entity),
    Timeout(Entity),
    Ping(Entity, u64),
    Latency(Entity, Duration),
    Disconnect(Entity),
    IncompatibleProtocol(Entity, u8),
    LastActivity(Entity, Instant),
    IncomingBatch(Entity, Vec<u8>),
    OutgoingBatch(Entity, Vec<u8>),
}

/// NetworkEvent can be used for handling various Minecraft related Login Process events
/// and to receive and send a Minecraft (Optionally Compressed & Encrypted) Packet Batch.
#[derive(Event)]
pub enum NetworkEvent {
    ConnectionRequest(Entity),
    ConnectionEstablished(Entity),
    IncomingPacket(Entity, Bytes),
    OutgoingPacket(Entity, Bytes),
}
