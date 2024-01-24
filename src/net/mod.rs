use bevy::ecs::{
    entity::Entity,
    event::{EventReader, EventWriter},
    system::{Commands, Query, Res, ResMut},
};
use binary::prefixed::UnsizedBytes;
use log::debug;

use self::{
    socket::{Mappings, RakSocket, SocketInfo},
    stream::{NetworkStatus, RakStream},
};
use crate::{
    generic::events::RakNetEvent,
    protocol::{
        mcpe::{
            BroadcastGamemode, MaxPlayers, MinecraftProtocol, MinecraftVersion, OnlinePlayers,
            PrimaryMotd, SecondaryMotd, StatusResource,
        },
        message::Message,
        reliability::Reliability,
        RAKNET_TIMEOUT,
    },
};
use std::io::Write;

pub mod socket;
pub mod stream;

/// This system is responsible for checking any outlived connections and sends a timeout to the connections
/// that don't respond for more than a specific time period.
pub fn check_timeout(query: Query<(Entity, &NetworkStatus)>, mut ev: EventWriter<RakNetEvent>) {
    for (entity, status) in query.iter() {
        if status.last_activity.elapsed().as_millis() > RAKNET_TIMEOUT {
            ev.send(RakNetEvent::Timeout(entity))
        }
    }
}

/// This system is responsible for building the MCPE Status that is sent in the Unconnected Pong message.
pub fn server_update_status(
    query: Query<(
        &PrimaryMotd,
        &SecondaryMotd,
        &OnlinePlayers,
        &MaxPlayers,
        &MinecraftProtocol,
        &MinecraftVersion,
        &BroadcastGamemode,
        &SocketInfo,
    )>,
    mut status: ResMut<StatusResource>,
) {
    let query = query.get_single().unwrap();
    status.bytes.clear();

    if let Err(e) = write!(
        &mut status.bytes,
        "MCPE;{};{};{};{};{};{};{};{};1;{};",
        query.0.get(),
        query.4.get(),
        query.5.get(),
        query.2.get(),
        query.3.get(),
        query.7.guid,
        query.1.get(),
        query.6.get(),
        query.7.addr.port()
    ) {
        debug!("[Status Error]: {}", e.to_string());
        return;
    }
}

/// This system is responsible for reading for any messages from the UdpSocket. It handles all the Unconnected Messages
/// and internal Connected Messages immediately while it writes an event for any Game Packets received.
pub fn server_read_udp(
    mut query: Query<&mut RakStream>,
    mut server: Query<(&mut RakSocket, &mut Mappings, &SocketInfo)>,
    mut ev: EventWriter<RakNetEvent>,
    mut commands: Commands,
    status: Res<StatusResource>,
) {
    let (mut socket, mut mappings, info) = server.get_single_mut().unwrap();
    let status = match std::str::from_utf8(&status.bytes) {
        Ok(status) => status,
        Err(e) => {
            debug!("[Status Error]: {}", e.to_string());
            return;
        }
    };

    let udp = socket.udp.clone();
    if let Ok((len, addr)) = udp.recv_from(&mut socket.read_buf) {
        if socket.is_blocked(addr, &mut mappings) {
            return;
        }

        if socket.check_packet_spam(addr, &mut mappings) {
            return;
        }

        if socket.handle_connected_message(addr, len, &mut query, &mut ev, &mut mappings) {
            return;
        }

        if let Err(e) = socket.handle_unconnected_message(
            addr,
            len,
            status,
            &mut commands,
            &mut ev,
            &info,
            &mut mappings,
        ) {
            socket.check_invalid_packets(addr, &mut mappings);
            debug!("[Network Error]: {}", e.to_string());
        }
    }
}

/// This system is responsible for reading for any messages from the UdpSocket. It handles all the Unconnected Messages
/// and internal Connected Messages immediately while it writes an event for any Game Packets received.
pub fn client_read_udp(
    mut client: Query<(Entity, &mut RakSocket, &mut RakStream)>,
    mut ev: EventWriter<RakNetEvent>,
) {
    let (entity, mut socket, mut stream) = client.get_single_mut().unwrap();

    let udp = socket.udp.clone();
    if let Ok(len) = udp.recv(&mut socket.read_buf) {
        if let Err(e) = stream.decode(&socket.read_buf[..len], &mut ev, entity) {
            debug!("[Network Error]: {}", e.to_string());
        }
    }
}

/// This system is responsible for flushing receipts for those sequence numbers that we did receive ACK
/// and for those we didn't (NACK).
pub fn flush_receipts(mut query: Query<&mut RakStream>) {
    for mut stream in query.iter_mut() {
        stream.flush_receipts();
    }
}

/// This system is responsible for flushing of datagrams that we have written so far for all connections
/// to the other end of the connection.
pub fn flush_batch(mut query: Query<&mut RakStream>) {
    for mut stream in query.iter_mut() {
        stream.try_flush();
    }
}

/// This system is responsible for checking the connection states, updating latencies, pings, etc.
pub fn connection_tick(
    mut ev: EventReader<RakNetEvent>,
    mut commands: Commands,
    mut query: Query<(&mut NetworkStatus, &mut RakStream)>,
) {
    for event in ev.read() {
        match event {
            RakNetEvent::Disconnect(entity) => {
                debug!(
                    "[Network] Entity ID {:?} has been disconnected from the server",
                    entity.index(),
                );

                commands.entity(*entity).despawn();
            }
            RakNetEvent::Latency(entity, latency) => {
                let (mut status, _) = query.get_mut(*entity).unwrap();
                status.latency = *latency;
            }
            RakNetEvent::Ping(entity, ping) => {
                let (mut status, _) = query.get_mut(*entity).unwrap();
                status.ping = *ping;
            }
            RakNetEvent::LastActivity(entity, last_activity) => {
                let (mut status, _) = query.get_mut(*entity).unwrap();
                status.last_activity = *last_activity;
            }
            RakNetEvent::OutgoingBatch(entity, bytes) => {
                let (_, mut conn) = query.get_mut(*entity).unwrap();
                let message = Message::GamePacket {
                    data: UnsizedBytes::new(&bytes),
                };

                conn.encode(message, Reliability::ReliableOrdered);
            }
            _ => {}
        }
    }
}
