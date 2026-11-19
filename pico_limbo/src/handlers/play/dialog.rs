//! Dialog relay between a proxy and the player.
//!
//! PicoLimbo has no plugins, so it cannot decide *what* to ask. A proxy plugin builds a vanilla
//! dialog (1.21.6+) as JSON and sends it over the `haventriton:dialog` plugin channel. PicoLimbo
//! converts it to NBT and shows it. When the player presses a button whose action is `custom`,
//! PicoLimbo sends the action id back to the proxy over the same channel.
//!
//! Wire format (both directions): `[version: u8][action: u8][body: bytes until the end]`
//!
//! | direction        | action | body                                   |
//! |------------------|--------|----------------------------------------|
//! | proxy -> limbo   | 1 SHOW | dialog definition as UTF-8 JSON object |
//! | limbo -> proxy   | 2 CLICK| id of the custom click action (UTF-8)  |
//! | limbo -> proxy   | 3 SHOWN| empty, the dialog was sent to the client |
//! | limbo -> proxy   | 4 UNSUPPORTED | empty, the client cannot show it |

use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::client_bound_raw_plugin_message_packet::PlayClientBoundRawPluginMessagePacket;
use minecraft_packets::play::custom_click_action_packet::CustomClickActionPacket;
use minecraft_packets::play::server_bound_plugin_message_packet::ServerBoundPluginMessagePacket;
use minecraft_packets::play::show_dialog_packet::ShowDialogPacket;
use minecraft_protocol::prelude::{Identifier, ProtocolVersion, State};
use pico_nbt::Value;
use tracing::{debug, warn};

const CHANNEL: &str = "haventriton:dialog";
const VERSION: u8 = 1;

const ACTION_SHOW: u8 = 1;
const ACTION_CLICK: u8 = 2;
const ACTION_SHOWN: u8 = 3;
const ACTION_UNSUPPORTED: u8 = 4;

/// A dialog is a few kilobytes at most; refuse anything absurd.
const MAX_DIALOG_BYTES: usize = 64 * 1024;
const MAX_ACTION_ID_BYTES: usize = 256;

impl PacketHandler for ServerBoundPluginMessagePacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch, PacketHandlerError> {
        let mut batch = Batch::new();

        if self.channel() != CHANNEL {
            return Ok(batch);
        }
        let Some((action, body)) = split_message(self.data()) else {
            return Ok(batch);
        };
        if action != ACTION_SHOW {
            return Ok(batch);
        }

        if !can_show_dialogs(client_state) {
            batch.queue(|| reply(ACTION_UNSUPPORTED, &[]));
            return Ok(batch);
        }

        match dialog_from_json(body) {
            Ok(dialog) => {
                debug!("Showing a dialog to {}", client_state.get_username());
                batch.queue(move || PacketRegistry::ShowDialog(ShowDialogPacket::inline(dialog)));
                batch.queue(|| reply(ACTION_SHOWN, &[]));
            }
            Err(reason) => {
                warn!(
                    "Rejected a dialog for {}: {reason}",
                    client_state.get_username()
                );
                batch.queue(|| reply(ACTION_UNSUPPORTED, &[]));
            }
        }
        Ok(batch)
    }
}

impl PacketHandler for CustomClickActionPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch, PacketHandlerError> {
        let mut batch = Batch::new();
        let id = self.id();
        if id.len() > MAX_ACTION_ID_BYTES {
            return Ok(batch);
        }
        debug!("{} clicked dialog action {id}", client_state.get_username());
        batch.queue(move || reply(ACTION_CLICK, id.as_bytes()));
        Ok(batch)
    }
}

/// Dialogs exist since 1.21.6 and the packet is only valid in the play state.
fn can_show_dialogs(client_state: &ClientState) -> bool {
    client_state.clientbound_state() == State::Play
        && client_state
            .protocol_version()
            .is_after_inclusive(ProtocolVersion::V1_21_6)
}

/// Splits `[version][action][body...]`. Returns `None` for an unknown version or a short message.
fn split_message(data: &[u8]) -> Option<(u8, &[u8])> {
    let (&version, rest) = data.split_first()?;
    if version != VERSION {
        return None;
    }
    let (&action, body) = rest.split_first()?;
    Some((action, body))
}

fn dialog_from_json(body: &[u8]) -> Result<Value, String> {
    if body.len() > MAX_DIALOG_BYTES {
        return Err(format!("dialog is too large ({} bytes)", body.len()));
    }
    let json: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    if !json.is_object() {
        return Err("dialog must be a JSON object".to_string());
    }
    pico_nbt::json_to_nbt(json).map_err(|e| e.to_string())
}

fn reply(action: u8, body: &[u8]) -> PacketRegistry {
    let mut data = Vec::with_capacity(body.len() + 2);
    data.push(VERSION);
    data.push(action);
    data.extend_from_slice(body);
    PacketRegistry::PlayClientBoundRawPluginMessage(PlayClientBoundRawPluginMessagePacket::new(
        Identifier::new_unchecked("haventriton", "dialog"),
        data,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_reads_version_action_and_body() {
        let data = [VERSION, ACTION_SHOW, b'{', b'}'];
        let (action, body) = split_message(&data).unwrap();
        assert_eq!(action, ACTION_SHOW);
        assert_eq!(body, b"{}");
    }

    #[test]
    fn split_message_rejects_unknown_version_and_short_messages() {
        assert!(split_message(&[]).is_none());
        assert!(split_message(&[VERSION]).is_none());
        assert!(split_message(&[VERSION + 1, ACTION_SHOW]).is_none());
    }

    #[test]
    fn dialog_must_be_a_json_object() {
        assert!(dialog_from_json(b"[]").is_err());
        assert!(dialog_from_json(b"\"text\"").is_err());
        assert!(dialog_from_json(b"not json").is_err());
    }

    #[test]
    fn dialog_json_becomes_a_compound() {
        let json =
            br#"{"type":"minecraft:multi_action","title":"Hi","can_close_with_escape":false}"#;
        let value = dialog_from_json(json).unwrap();
        assert!(matches!(value, Value::Compound(_)));
    }

    #[test]
    fn oversized_dialog_is_rejected() {
        let body = vec![b' '; MAX_DIALOG_BYTES + 1];
        assert!(dialog_from_json(&body).is_err());
    }
}
