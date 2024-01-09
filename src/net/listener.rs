use bevy::ecs::event::EventWriter;
use bevy::ecs::system::{ResMut, Resource};
use bytes::BytesMut;
use io::Result;

use std::collections::HashMap;
use std::{
    io::{self},
    net::{SocketAddr, UdpSocket},
    sync::Arc,
};

use crate::generic::events::{BlockReason, RakNetEvent};
use crate::generic::timestamp;
use crate::protocol::{MAX_DATAGRAMS_PER_SECOND, RAKNET_BLOCK_DUR};

use super::stream::RakStream;

#[derive(Resource)]
pub struct RakListener {
    socket: Arc<UdpSocket>,
    addr: SocketAddr,

    read_buf: BytesMut,
    write_buf: BytesMut,

    connections: HashMap<SocketAddr, RakStream>,
    blocked: HashMap<SocketAddr, u64>,

    packets_per_sec: HashMap<SocketAddr, u8>,
    invalid_packets: HashMap<SocketAddr, u8>,
}

impl RakListener {
    pub fn bind(addr: SocketAddr) -> Result<Self> {
        let read_buf = BytesMut::zeroed(1500);
        let write_buf = BytesMut::with_capacity(1500);
        let connections = HashMap::new();
        let blocked = HashMap::new();
        let packets_per_sec = HashMap::new();
        let invalid_packets = HashMap::new();

        match UdpSocket::bind(addr) {
            Ok(socket) => {
                socket.set_nonblocking(true).unwrap();

                Ok(Self {
                    socket: socket.into(),
                    addr,
                    read_buf,
                    write_buf,
                    connections,
                    blocked,
                    packets_per_sec,
                    invalid_packets,
                })
            }
            Err(e) => Err(e),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

pub(crate) fn handle_raknet_packet(
    mut listener: ResMut<RakListener>,
    mut ev: EventWriter<RakNetEvent>,
) {
    let socket = &listener.socket.clone();

    if let Ok((len, addr)) = socket.recv_from(&mut listener.read_buf) {
        if let Some(expiry) = listener.blocked.get(&addr) {
            if expiry > &timestamp() {
                return;
            }

            listener.blocked.remove(&addr);
        }

        let packets = listener.packets_per_sec.get(&addr).unwrap_or(&0) + 1;

        if packets == MAX_DATAGRAMS_PER_SECOND {
            listener.packets_per_sec.remove(&addr);
            listener
                .blocked
                .insert(addr, timestamp() + RAKNET_BLOCK_DUR.as_secs());

            ev.send(RakNetEvent::Blocked(
                addr,
                RAKNET_BLOCK_DUR,
                BlockReason::PacketSpam,
            ));
            return;
        }

        listener.packets_per_sec.insert(addr, packets);

        let reader = &listener.read_buf[..len];

        ev.send(RakNetEvent::Ping(addr));
        println!("Buffer: {:?}", reader);
    }
}
