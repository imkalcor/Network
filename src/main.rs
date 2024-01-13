use std::net::SocketAddr;

use bevy::{prelude::*, time::common_conditions::on_timer};
use commons::logger::init_logger;
use generic::events::{NetworkEvent, RakNetEvent};
use log::LevelFilter;
use net::{listener::Listener, system_check_timeout, system_read_from_udp, system_write_to_udp};
use protocol::RAKNET_CHECK_TIMEOUT;

pub mod generic;
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
        app.add_systems(
            PreUpdate,
            system_check_timeout.run_if(on_timer(RAKNET_CHECK_TIMEOUT)),
        );
    }
}

fn main() {
    init_logger(LevelFilter::Trace);

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(NetworkServer::new("0.0.0.0:19132"))
        .run();
}
