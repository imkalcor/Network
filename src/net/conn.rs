use std::{
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};

use bevy::ecs::component::Component;
use bytes::BytesMut;

use crate::generic::events::DisconnectReason;

/// Network Information about a connected RakNet client.
#[derive(Component)]
pub struct NetworkInfo {
    pub last: Instant,
    pub latency: Duration,
    pub ping: u64,
}

#[derive(Component)]
pub struct RakNetEncoder {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    mtu_size: u32,

    sequence_number: u32,
    reliability_index: u32,
    order_index: u32,

    batch: BytesMut,
}

impl RakNetEncoder {
    pub fn encode(&mut self, bytes: &[u8]) {}

    pub fn disconnect(&mut self, reason: &DisconnectReason) {}
}

#[derive(Component)]
pub struct RakNetDecoder {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
}

impl RakNetDecoder {
    pub fn decode(&mut self, buffer: &[u8]) -> bool {
        return false;
    }
}

#[derive(Component)]
pub struct NetworkEncoder {}

#[derive(Component)]
pub struct NetworkDecoder {}
