use bevy::ecs::{
    entity::Entity,
    event::{EventReader, EventWriter},
    system::{Query, ResMut},
};

use crate::{
    generic::events::{DisconnectReason, NetworkEvent, RakNetEvent},
    protocol::RAKNET_TIMEOUT,
};

use self::{
    conn::{NetworkDecoder, NetworkEncoder, NetworkInfo, RakNetDecoder, RakNetEncoder},
    listener::Listener,
};

pub mod conn;
pub mod listener;

/// This system is responsible for checking any outlived connections and sends a timeout to the connections
/// that don't respond for more than a specific time period.
pub fn system_check_outlived_connections(
    query: Query<(Entity, &NetworkInfo)>,
    mut ev: EventWriter<RakNetEvent>,
) {
    for (entity, activity) in query.iter() {
        if activity.last.elapsed().as_millis() > RAKNET_TIMEOUT {
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
    mut query: Query<&mut RakNetDecoder>,
    mut listener: ResMut<Listener>,
    mut ev: EventWriter<RakNetEvent>,
) {
    if let Ok((len, addr)) = listener.try_recv() {
        if listener.is_blocked(addr) {
            return;
        }

        if listener.check_packet_spam(addr, &mut ev) {
            return;
        }

        if !listener.try_parse_connected_message(addr, &mut query) {
            listener.check_invalid_packets(addr, &mut ev);
            return;
        }

        if !listener.try_parse_unconnected_message(addr, len, &mut ev) {
            listener.check_invalid_packets(addr, &mut ev);
            return;
        }
    }
}

/// This system is responsible for flushing and batching of any messages to the UdpSocket. It handles all outgoing game packets
/// by writing them over the Udp Network.
pub fn system_write_to_udp(mut query: Query<&mut RakNetEncoder>, mut ev: EventReader<RakNetEvent>) {
    for event in ev.read() {
        match event {
            RakNetEvent::OutgoingBatch(entity, buffer) => {
                if let Ok(mut encoder) = query.get_mut(*entity) {
                    encoder.encode(&buffer)
                }
            }
            RakNetEvent::Disconnect(entity, reason) => {
                if let Ok(mut encoder) = query.get_mut(*entity) {
                    encoder.disconnect(reason)
                }
            }
            _ => {}
        }
    }
}

/// This system is responsible for deserializing, decrypting and decompressing a Minecraft Game Packet batch received
/// from a RakNet connection.
pub fn system_read_from_raknet(
    mut query: Query<&mut NetworkDecoder>,
    mut raknet: EventReader<RakNetEvent>,
    mut network: EventWriter<NetworkEvent>,
) {
}

/// This system is resposible for flushing and batching Minecraft Packets and writes them to the RakNet connection.
pub fn system_write_to_raknet(
    mut query: Query<&mut NetworkEncoder>,
    mut network: EventReader<NetworkEvent>,
    mut raknet: EventWriter<RakNetEvent>,
) {
}
