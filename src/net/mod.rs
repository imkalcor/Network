use bevy::ecs::{
    entity::Entity,
    event::{EventReader, EventWriter},
    system::{Commands, Query, ResMut},
};
use binary::prefixed::UnsizedBytes;
use log::debug;

use self::{
    conn::{NetworkInfo, RakStream},
    listener::Listener,
};
use crate::{
    generic::events::{DisconnectReason, RakNetEvent},
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

pub mod conn;
pub mod listener;

/// This system is responsible for checking any outlived connections and sends a timeout to the connections
/// that don't respond for more than a specific time period.
pub fn system_check_timeout(
    query: Query<(Entity, &NetworkInfo)>,
    mut ev: EventWriter<RakNetEvent>,
) {
    for (entity, activity) in query.iter() {
        if activity.last_activity.elapsed().as_millis() > RAKNET_TIMEOUT {
            ev.send(RakNetEvent::Disconnect(
                entity,
                DisconnectReason::ClientTimeout,
            ))
        }
    }
}

/// This system is responsible for reading for any messages from the UdpSocket. It handles all the Unconnected Messages
/// and internal Connected Messages immediately while it writes an event for any Game Packets received.
pub fn system_decode_incoming(
    mut query: Query<(&mut RakStream, &mut NetworkInfo)>,
    mut listener: Query<&mut Listener>,
    mut ev: EventWriter<RakNetEvent>,
    mut commands: Commands,
    mut status_res: ResMut<StatusResource>,
    status: Query<(
        &PrimaryMotd,
        &SecondaryMotd,
        &OnlinePlayers,
        &MaxPlayers,
        &MinecraftProtocol,
        &MinecraftVersion,
        &BroadcastGamemode,
    )>,
) {
    let mut listener = listener.get_single_mut().unwrap();
    let status = status.get_single().unwrap();
    let status_bytes = &mut status_res.bytes;

    if let Ok((len, addr)) = listener.try_recv() {
        if listener.is_blocked(addr) {
            return;
        }

        if listener.check_packet_spam(addr, &mut ev) {
            return;
        }

        if listener.handle_connected_message(addr, len, &mut query, &mut ev) {
            return;
        }

        if let Err(e) = write!(
            status_bytes,
            "MCPE;{};{};{};{};{};{};{};{};1;{};",
            status.0.get(),
            status.4.get(),
            status.5.get(),
            status.2.get(),
            status.3.get(),
            listener.guid,
            status.1.get(),
            status.6.get(),
            listener.addr.port()
        ) {
            debug!("[Network Error]: {}", e.to_string());
            return;
        }

        let status = match std::str::from_utf8(&status_bytes) {
            Ok(status) => status,
            Err(e) => {
                debug!("[Network Error]: {}", e.to_string());
                return;
            }
        };

        if let Err(e) = listener.handle_unconnected_message(addr, len, status, &mut commands) {
            debug!("[Network Error]: {}", e.to_string());
            listener.check_invalid_packets(addr, &mut ev);
            return;
        }

        status_bytes.clear();
    }
}

/// This system is responsible for flushing receipts for those sequence numbers that we did receive ACK
/// and for those we didn't (NACK).
pub fn system_flush_receipts(mut query: Query<&mut RakStream>) {
    for mut stream in query.iter_mut() {
        stream.flush_receipts();
    }
}

/// This system is responsible for encoding outgoing datagrams to the connection's internal writing buffer. They
/// are then flushed periodically by the `system_flush_to_udp` system.
pub fn system_encode_outgoing(mut query: Query<&mut RakStream>, mut ev: EventReader<RakNetEvent>) {
    for event in ev.read() {
        match event {
            RakNetEvent::S2CGamePacket(entity, bytes) => {
                let mut conn = query.get_mut(*entity).unwrap();
                let message = Message::GamePacket {
                    data: UnsizedBytes::new(&bytes),
                };

                conn.encode(message, Reliability::ReliableOrdered);
            }
            _ => {}
        }
    }
}

/// This system is responsible for flushing of datagrams that we have written so far for all connections
/// to the other end of the connection.
pub fn system_flush_to_udp(mut query: Query<&mut RakStream>) {
    for mut stream in query.iter_mut() {
        stream.try_flush();
    }
}

/// This system is responsible for checking the connection states, such as logging when a connection gets blocked,
/// handling disconnection of a connection, etc.
pub fn system_check_connections(
    mut ev: EventReader<RakNetEvent>,
    mut conn: Query<&mut RakStream>,
    mut commands: Commands,
) {
    for event in ev.read() {
        match event {
            RakNetEvent::Disconnect(entity, reason) => {
                if let Ok(mut conn) = conn.get_mut(*entity) {
                    debug!(
                        "[Network] Entity ID {:?} has been disconnected due to {:?}",
                        entity.index(),
                        reason
                    );

                    conn.disconnect();
                    commands.entity(*entity).despawn();
                }
            }
            RakNetEvent::Blocked(addr, dur, reason) => {
                debug!("Blocked {:?} for {:?} - Duration: {:?}", addr, reason, dur);
            }
            _ => {}
        }
    }
}
