use minecraft_protocol::prelude::*;
use pico_nbt::Value;

/// Clientbound `minecraft:show_dialog` in the play state (1.21.6+).
#[derive(PacketOut)]
pub struct ShowDialogPacket {
    /// Holder prefix. `0` means "the dialog follows inline" instead of a registry id plus one.
    holder: VarInt,
    dialog: Value,
}

impl ShowDialogPacket {
    /// Sends a dialog definition (a compound tag) inline, without needing a registry entry.
    pub fn inline(dialog: Value) -> Self {
        Self {
            holder: VarInt::new(0),
            dialog,
        }
    }
}
