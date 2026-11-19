use minecraft_protocol::prelude::*;

/// Serverbound `minecraft:custom_click_action` (1.21.6+).
///
/// Sent by the client when the player presses a dialog button (or clicks a text component)
/// whose action is `custom`.
#[derive(PacketIn)]
pub struct CustomClickActionPacket {
    id: Identifier,
    // The optional NBT payload that follows is ignored: PicoLimbo only relays the id.
}

impl CustomClickActionPacket {
    /// The action id formatted as `namespace:path`.
    pub fn id(&self) -> String {
        self.id.to_string()
    }
}
