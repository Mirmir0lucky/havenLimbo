use minecraft_protocol::prelude::*;

/// Clientbound `minecraft:custom_payload` in the play state with an arbitrary,
/// not length-prefixed payload.
///
/// [`PlayClientBoundPluginMessagePacket`](super::client_bound_plugin_message_packet::PlayClientBoundPluginMessagePacket)
/// is kept for the server brand, which is a length-prefixed string.
#[derive(PacketOut)]
pub struct PlayClientBoundRawPluginMessagePacket {
    channel: Identifier,
    data: Vec<u8>,
}

impl PlayClientBoundRawPluginMessagePacket {
    pub fn new(channel: Identifier, data: Vec<u8>) -> Self {
        Self { channel, data }
    }
}
