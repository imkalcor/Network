use bevy::{prelude::*, time::common_conditions::on_timer};
use commons::logger::init_logger;
use generic::events::{NetworkEvent, RakNetEvent};
use log::LevelFilter;
use net::{
    listener::{Listener, ServerBundle},
    system_check_connections, system_check_timeout, system_decode_incoming, system_encode_outgoing,
    system_flush_receipts, system_flush_to_udp,
};
use protocol::{
    mcpe::{
        BroadcastGamemode, MaxPlayers, MinecraftProtocol, MinecraftVersion, OnlinePlayers,
        PrimaryMotd, SecondaryMotd, StatusResource,
    },
    RAKNET_CHECK_TIMEOUT, RAKNET_TPS,
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

        app.world.spawn(ServerBundle {
            listener: Listener::new(self.addr).unwrap(),
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

    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(NetworkServer::new("0.0.0.0:19132"))
        .run();
}
