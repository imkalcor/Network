pub mod listener;
pub mod stream;

use bevy::ecs::event::EventReader;

use crate::generic::events::RakNetEvent;

pub(crate) fn handle_events(mut ev: EventReader<RakNetEvent>) {
    for event in ev.read() {
        match event {
            RakNetEvent::Ping(addr) => println!("[RakNet] {:?} pinged", addr),
            RakNetEvent::Blocked(addr, duration, reason) => {
                println!(
                    "[RakNet] Blocking {:?} for {:?} - Reason: {:?}",
                    addr, duration, reason
                )
            }
            RakNetEvent::Connecting(addr) => println!("[RakNet] {:?} is connecting.", addr),
            RakNetEvent::Connected(addr, id) => println!(
                "[RakNet] {:?} has connected with Entity ID {:?}",
                addr,
                id.index()
            ),
            RakNetEvent::GamePacket(id, addr, buffer) => {
                println!(
                    "[RakNet] Game Packet {:?} from {:?} (ID: {:?})",
                    buffer.to_vec(),
                    addr,
                    id.index()
                )
            }
            RakNetEvent::Disconnect(id, addr, reason) => println!(
                "[RakNet] {:?} (ID {:?}) has disconnected - {:?}",
                addr,
                id.index(),
                reason
            ),
        }
    }
}
