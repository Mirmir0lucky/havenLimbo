use minecraft_protocol::prelude::*;

/// Serverbound `minecraft:player_loaded`: the client has finished loading the world.
#[derive(PacketIn)]
pub struct PlayerLoadedPacket {}
