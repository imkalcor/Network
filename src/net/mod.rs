use bevy::ecs::{
    entity::Entity,
    event::{EventReader, EventWriter},
    system::{Commands, Query, ResMut},
};
use binary::prefixed::UnsizedBytes;
use log::debug;

use crate::{
    generic::events::{DisconnectReason, RakNetEvent},
    protocol::{message::Message, reliability::Reliability, RAKNET_TIMEOUT},
};

use self::{
    conn::{NetworkInfo, RakStream},
    listener::Listener,
};

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
pub fn system_read_from_udp(
    mut query: Query<(&mut RakStream, &mut NetworkInfo)>,
    mut listener: ResMut<Listener>,
    mut ev: EventWriter<RakNetEvent>,
    mut commands: Commands,
) {
    if let Ok((len, addr)) = listener.try_recv() {
        if listener.is_blocked(addr) {
            return;
        }

        if listener.check_packet_spam(addr, &mut ev) {
            return;
        }

        match listener.handle_connected_message(addr, len, &mut query, &mut ev) {
            Ok(result) => {
                if result {
                    return;
                }
            }
            Err(e) => {
                debug!("[Network Error]: {}", e.to_string());
                listener.check_invalid_packets(addr, &mut ev);
                return;
            }
        }

        if let Err(e) = listener.handle_unconnected_message(addr, len, &mut commands) {
            debug!("[Network Error]: {}", e.to_string());
            listener.check_invalid_packets(addr, &mut ev);
            return;
        }
    }
}

/// This system is responsible for flushing and batching of any messages to the UdpSocket. It handles all outgoing game packets
/// by writing them over the Udp Network.
pub fn system_write_to_udp(mut query: Query<&mut RakStream>, mut ev: EventReader<RakNetEvent>) {
    for event in ev.read() {
        match event {
            RakNetEvent::S2CGamePacket(entity, bytes) => {
                let mut conn = query.get_mut(*entity).unwrap();
                let message = Message::GamePacket {
                    data: UnsizedBytes::new(&bytes),
                };

                conn.encode(message, Reliability::ReliableOrdered);
            }
            RakNetEvent::Blocked(addr, dur, reason) => {
                debug!("Blocked {:?} for {:?} - Duration: {:?}", addr, reason, dur);
            }
            _ => {}
        }
    }

    for mut stream in query.iter_mut() {
        stream.try_flush();
    }
}
