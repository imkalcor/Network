use bevy::{prelude::*, time::common_conditions::on_timer};
use commons::logger::init_logger;
use generic::events::{NetworkEvent, RakNetEvent};
use log::LevelFilter;
use net::{
    check_timeout, client_read_udp, connection_tick, flush_batch, flush_receipts, server_read_udp,
    server_update_status,
    socket::{RakSocket, ServerBundle},
};
use protocol::{mcpe::StatusResource, RAKNET_CHECK_TIMEOUT, RAKNET_TPS};

pub mod generic;
pub mod net;
pub mod protocol;

pub struct NetworkServer {
    addr: String,
}

impl NetworkServer {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
        }
    }
}

impl Plugin for NetworkServer {
    fn build(&self, app: &mut App) {
        app.add_event::<RakNetEvent>();
        app.add_event::<NetworkEvent>();
        app.add_systems(PreUpdate, server_read_udp);
        app.add_systems(PreUpdate, flush_receipts.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(PreUpdate, flush_batch.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(
            PreUpdate,
            check_timeout.run_if(on_timer(RAKNET_CHECK_TIMEOUT)),
        );
        app.add_systems(PreUpdate, connection_tick);
        app.add_systems(Update, server_update_status.run_if(on_timer(RAKNET_TPS)));
        app.world.spawn(ServerBundle::new(&self.addr));
        app.insert_resource(StatusResource::new());
    }
}

pub struct NetworkClient {
    addr: String,
}

impl NetworkClient {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
        }
    }
}

impl Plugin for NetworkClient {
    fn build(&self, app: &mut App) {
        app.add_event::<RakNetEvent>();
        app.add_event::<NetworkEvent>();
        app.add_systems(PreUpdate, client_read_udp);
        app.add_systems(PreUpdate, flush_receipts.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(PreUpdate, flush_batch.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(
            PreUpdate,
            check_timeout.run_if(on_timer(RAKNET_CHECK_TIMEOUT)),
        );
        app.add_systems(PreUpdate, connection_tick);

        RakSocket::connect(&self.addr, &mut app.world).unwrap();
    }
}

pub struct NetworkProxy {
    addr: String,
}

impl NetworkProxy {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
        }
    }
}

impl Plugin for NetworkProxy {
    fn build(&self, app: &mut App) {
        app.add_event::<RakNetEvent>();
        app.add_event::<NetworkEvent>();
        app.add_systems(PreUpdate, server_read_udp);
        app.add_systems(PreUpdate, client_read_udp);
        app.add_systems(PreUpdate, flush_receipts.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(PreUpdate, flush_batch.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(
            PreUpdate,
            check_timeout.run_if(on_timer(RAKNET_CHECK_TIMEOUT)),
        );
        app.add_systems(PreUpdate, connection_tick);
        app.add_systems(Update, server_update_status.run_if(on_timer(RAKNET_TPS)));
        app.world.spawn(ServerBundle::new(&self.addr));
        app.insert_resource(StatusResource::new());

        RakSocket::connect(&self.addr, &mut app.world).unwrap();
    }
}

fn main() {
    init_logger(LevelFilter::Trace);

    let mut task_pool_options = TaskPoolOptions::default();
    task_pool_options.io.min_threads = 0;
    task_pool_options.io.max_threads = 0;
    task_pool_options.io.percent = 0.0;

    App::new()
        .add_plugins(MinimalPlugins.set(TaskPoolPlugin {
            task_pool_options: task_pool_options,
        }))
        .add_plugins(NetworkServer::new("127.0.0.1:19132"))
        .run();
}
