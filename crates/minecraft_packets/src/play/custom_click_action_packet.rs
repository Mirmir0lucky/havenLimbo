use minecraft_protocol::prelude::*;

/// Serverbound `minecraft:custom_click_action` (1.21.6+).
///
/// Sent by the client when the player presses a dialog button (or clicks a text component)
/// whose action is `custom`. For dialog buttons the payload holds the values of the dialog's
/// input controls, each stored under its `key`.
#[derive(PacketIn)]
pub struct CustomClickActionPacket {
    id: Identifier,
    /// The optional NBT payload, undecoded (it is the rest of the packet).
    payload: Vec<u8>,
}

impl CustomClickActionPacket {
    /// The action id formatted as `namespace:path`.
    pub fn id(&self) -> String {
        self.id.to_string()
    }

    /// The raw bytes that follow the id: empty, or an optional nameless NBT tag.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
