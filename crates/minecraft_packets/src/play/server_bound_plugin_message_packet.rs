use minecraft_protocol::prelude::*;

/// Serverbound `minecraft:custom_payload` in the play state.
///
/// A proxy (or a client) sends a plugin message on a channel. PicoLimbo only cares about the
/// channels it knows, everything else is ignored by the handler.
#[derive(PacketIn)]
pub struct ServerBoundPluginMessagePacket {
    channel: Identifier,
    /// The rest of the packet, without any length prefix.
    data: Vec<u8>,
}

impl ServerBoundPluginMessagePacket {
    /// The channel formatted as `namespace:path`.
    pub fn channel(&self) -> String {
        self.channel.to_string()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
