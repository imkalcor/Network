use std::{net::SocketAddr, time::Duration};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use generic::events::RakNetEvent;
use net::{
    handle_events,
    listener::{handle_raknet_packet, RakListener},
};

pub mod generic;
pub mod net;
pub mod protocol;

pub struct NetworkServer {
    addr: SocketAddr,
}

impl NetworkServer {
    pub fn new(addr: &str) -> Self {
        match addr.parse::<SocketAddr>() {
            Ok(addr) => Self { addr },
            Err(e) => panic!("{:?}", e.to_string()),
        }
    }
}

impl Plugin for NetworkServer {
    fn build(&self, app: &mut App) {
        let listener = match RakListener::bind(self.addr) {
            Ok(listener) => listener,
            Err(_) => return,
        };

        app.insert_resource(listener);
        app.add_event::<RakNetEvent>();
        app.add_systems(PreUpdate, handle_raknet_packet);
        app.add_systems(PreUpdate, handle_events);
    }
}

fn main() {
    App::new()
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 100.0,
            ))),
        )
        .add_plugins(NetworkServer::new("127.0.0.1:19132"))
        .run();
}
