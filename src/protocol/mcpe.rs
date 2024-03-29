use bevy::ecs::{component::Component, system::Resource};
use bytes::BytesMut;

#[derive(Resource)]
pub struct StatusResource {
    pub bytes: BytesMut,
}

impl StatusResource {
    pub fn new() -> Self {
        let mut bytes = BytesMut::with_capacity(300);
        let status =
            "MCPE;RakNet;390;1.14.60;0;10;13253860892328930865;Blazingly fast;Survival;1;19132;";
        bytes.extend_from_slice(&status.as_bytes());

        Self { bytes }
    }
}

#[derive(Component)]
pub struct PrimaryMotd(String);

impl PrimaryMotd {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    pub fn get<'a>(&'a self) -> &'a str {
        &self.0
    }

    pub fn set(&mut self, value: &str) {
        self.0 = value.to_string()
    }
}

#[derive(Component)]
pub struct SecondaryMotd(String);

impl SecondaryMotd {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    pub fn get<'a>(&'a self) -> &'a str {
        &self.0
    }

    pub fn set(&mut self, value: &str) {
        self.0 = value.to_string()
    }
}

#[derive(Component)]
pub struct OnlinePlayers(u32);

impl OnlinePlayers {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn set(&mut self, value: u32) {
        self.0 = value
    }
}

#[derive(Component)]
pub struct MaxPlayers(u32);

impl MaxPlayers {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn set(&mut self, value: u32) {
        self.0 = value
    }
}

#[derive(Component)]
pub struct MinecraftProtocol(u32);

impl MinecraftProtocol {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn set(&mut self, value: u32) {
        self.0 = value
    }
}

#[derive(Component)]
pub struct MinecraftVersion(String);

impl MinecraftVersion {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    pub fn get<'a>(&'a self) -> &'a str {
        &self.0
    }

    pub fn set(&mut self, value: &str) {
        self.0 = value.to_string()
    }
}

#[derive(Component)]
pub struct BroadcastGamemode(String);

impl BroadcastGamemode {
    pub fn new(value: &str) -> Self {
        Self(value.to_string())
    }

    pub fn get<'a>(&'a self) -> &'a str {
        &self.0
    }

    pub fn set(&mut self, value: &str) {
        self.0 = value.to_string()
    }
}
