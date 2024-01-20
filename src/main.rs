use bevy::{prelude::*, time::common_conditions::on_timer};
use commons::logger::init_logger;
use generic::events::{NetworkEvent, RakNetEvent};
use log::LevelFilter;
use net::{
    listener::{Listener, ListenerInfo, ServerBundle},
    system_check_connections, system_check_timeout, system_decode_incoming, system_encode_outgoing,
    system_flush_receipts, system_flush_to_udp, system_update_status,
};
use protocol::{
    mcpe::{
        BroadcastGamemode, MaxPlayers, MinecraftProtocol, MinecraftVersion, OnlinePlayers,
        PrimaryMotd, SecondaryMotd, StatusResource,
    },
    RAKNET_CHECK_TIMEOUT, RAKNET_TPS, UPDATE_MCPE_STATUS,
};
use std::net::SocketAddr;

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
        app.add_event::<RakNetEvent>();
        app.add_event::<NetworkEvent>();
        app.add_systems(PreUpdate, system_decode_incoming);
        app.add_systems(
            PreUpdate,
            system_flush_receipts.run_if(on_timer(RAKNET_TPS)),
        );
        app.add_systems(PreUpdate, system_encode_outgoing);
        app.add_systems(PreUpdate, system_flush_to_udp.run_if(on_timer(RAKNET_TPS)));
        app.add_systems(
            PreUpdate,
            system_check_timeout.run_if(on_timer(RAKNET_CHECK_TIMEOUT)),
        );
        app.add_systems(PreUpdate, system_check_connections);
        app.add_systems(
            Update,
            system_update_status.run_if(on_timer(UPDATE_MCPE_STATUS)),
        );

        let listener = Listener::new(self.addr).unwrap();
        let guid = listener.guid;

        app.world.spawn(ServerBundle {
            listener: listener,
            info: ListenerInfo {
                addr: self.addr,
                guid,
            },
            primary_motd: PrimaryMotd::new("RakNet"),
            secondary_motd: SecondaryMotd::new("blazingly fast!"),
            online_players: OnlinePlayers::new(0),
            max_players: MaxPlayers::new(1000),
            gamemode: BroadcastGamemode::new("Survival"),
            protocol: MinecraftProtocol::new(600),
            version: MinecraftVersion::new("1.20.51"),
        });

        app.insert_resource(StatusResource::new());
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
        .add_plugins(NetworkServer::new("0.0.0.0:19132"))
        .run();
}
