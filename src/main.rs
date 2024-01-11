use std::{net::SocketAddr, time::Duration};

use bevy::{app::ScheduleRunnerPlugin, prelude::*};
use commons::logger::init_logger;
use generic::events::{NetworkEvent, RakNetEvent};
use log::LevelFilter;
use net::{
    listener::Listener, system_check_outlived_connections, system_read_from_raknet,
    system_read_from_udp, system_write_to_raknet, system_write_to_udp,
};

pub(crate) mod generic;
pub(crate) mod net;
pub(crate) mod protocol;

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
        let listener = Listener::new(self.addr).unwrap();

        app.insert_resource(listener);
        app.add_event::<RakNetEvent>();
        app.add_event::<NetworkEvent>();
        app.add_systems(PreUpdate, system_read_from_udp);
        app.add_systems(PreUpdate, system_write_to_udp);
        app.add_systems(PreUpdate, system_check_outlived_connections);
        app.add_systems(PreUpdate, system_read_from_raknet);
        app.add_systems(PreUpdate, system_write_to_raknet);
    }
}

fn main() {
    init_logger(LevelFilter::Trace);

    App::new()
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / 100.0,
            ))),
        )
        .add_plugins(NetworkServer::new("127.0.0.1:19132"))
        .run();
}
