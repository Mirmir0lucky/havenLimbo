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
//! | limbo -> proxy   | 5 SUBMIT| `{"id": "...", "fields": {...}}` as UTF-8 JSON: the id of the pressed button and the values of the dialog inputs |

use crate::server::batch::Batch;
use crate::server::client_state::ClientState;
use crate::server::packet_handler::{PacketHandler, PacketHandlerError};
use crate::server::packet_registry::PacketRegistry;
use crate::server_state::ServerState;
use minecraft_packets::play::client_bound_raw_plugin_message_packet::PlayClientBoundRawPluginMessagePacket;
use minecraft_packets::play::custom_click_action_packet::CustomClickActionPacket;
use minecraft_packets::play::player_loaded_packet::PlayerLoadedPacket;
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
const ACTION_SUBMIT: u8 = 5;

/// A dialog is a few kilobytes at most; refuse anything absurd.
const MAX_DIALOG_BYTES: usize = 64 * 1024;
const MAX_ACTION_ID_BYTES: usize = 256;
/// Input values (a username or password) are short; refuse anything larger.
const MAX_PAYLOAD_BYTES: usize = 8 * 1024;

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
            Ok(dialog) if client_state.is_player_loaded() => {
                debug!("Showing a dialog to {}", client_state.get_username());
                batch.queue(move || PacketRegistry::ShowDialog(ShowDialogPacket::inline(dialog)));
                batch.queue(|| reply(ACTION_SHOWN, &[]));
            }
            Ok(dialog) => {
                // The client closes any screen when its loading screen finishes, so wait for `player_loaded`.
                debug!(
                    "Holding a dialog for {} until the client has loaded",
                    client_state.get_username()
                );
                client_state.set_pending_dialog(dialog);
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

impl PacketHandler for PlayerLoadedPacket {
    fn handle(
        &self,
        client_state: &mut ClientState,
        _server_state: &ServerState,
    ) -> Result<Batch, PacketHandlerError> {
        let mut batch = Batch::new();
        client_state.set_player_loaded();
        if let Some(dialog) = client_state.take_pending_dialog() {
            debug!("Showing the held dialog to {}", client_state.get_username());
            batch.queue(move || PacketRegistry::ShowDialog(ShowDialogPacket::inline(dialog)));
            batch.queue(|| reply(ACTION_SHOWN, &[]));
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

        let fields = if self.payload().len() <= MAX_PAYLOAD_BYTES {
            payload_to_json(self.payload()).filter(|f| f.as_object().is_some_and(|o| !o.is_empty()))
        } else {
            None
        };
        if fields.is_none() && !self.payload().is_empty() {
            // Never log the bytes themselves: they can contain a password.
            warn!(
                "Could not read the input values of dialog action {id} from {} ({} payload bytes, first tag byte {})",
                client_state.get_username(),
                self.payload().len(),
                self.payload()[0]
            );
        }
        match fields {
            // Never log `fields`: it can contain a password.
            Some(fields) => {
                let body = serde_json::json!({ "id": id, "fields": fields }).to_string();
                batch.queue(move || reply(ACTION_SUBMIT, body.as_bytes()));
            }
            None => batch.queue(move || reply(ACTION_CLICK, id.as_bytes())),
        }
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

/// Decodes the optional nameless NBT tag of a `custom_click_action` into JSON.
///
/// Accepts both an "end tag means nothing" encoding and a leading presence boolean, so it keeps
/// working whichever the client uses. Returns `None` when there is no compound or it is malformed.
fn payload_to_json(bytes: &[u8]) -> Option<serde_json::Value> {
    let bytes = match bytes {
        [1, 10, ..] => &bytes[1..],
        _ => bytes,
    };
    let mut reader = NbtCursor { bytes, pos: 0 };
    if reader.u8()? != 10 {
        return None;
    }
    reader.compound(0)
}

struct NbtCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl NbtCursor<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))
    }

    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<String> {
        let len = usize::from(self.u16()?);
        let raw = self.take(len)?;
        Some(String::from_utf8_lossy(raw).into_owned())
    }

    fn compound(&mut self, depth: u32) -> Option<serde_json::Value> {
        if depth > 8 {
            return None;
        }
        let mut map = serde_json::Map::new();
        loop {
            let tag = self.u8()?;
            if tag == 0 {
                return Some(serde_json::Value::Object(map));
            }
            let name = self.string()?;
            map.insert(name, self.value(tag, depth + 1)?);
        }
    }

    fn value(&mut self, tag: u8, depth: u32) -> Option<serde_json::Value> {
        use serde_json::Value as J;
        Some(match tag {
            1 => J::from(i64::from(self.u8()? as i8)),
            2 => J::from(i64::from(self.u16()? as i16)),
            3 => J::from(i64::from(self.i32()?)),
            4 => {
                let b = self.take(8)?;
                J::from(i64::from_be_bytes(b.try_into().ok()?))
            }
            5 => {
                let b = self.take(4)?;
                J::from(f64::from(f32::from_be_bytes(b.try_into().ok()?)))
            }
            6 => {
                let b = self.take(8)?;
                J::from(f64::from_be_bytes(b.try_into().ok()?))
            }
            8 => J::from(self.string()?),
            10 => self.compound(depth)?,
            // Lists and arrays never appear in the controls we use.
            _ => return None,
        })
    }
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
    fn payload_with_string_fields_becomes_json() {
        // compound { password: "abc", confirm: "abc" }
        let mut data = vec![10];
        for (k, v) in [("password", "abc"), ("confirm", "abc")] {
            data.push(8);
            data.extend_from_slice(&(k.len() as u16).to_be_bytes());
            data.extend_from_slice(k.as_bytes());
            data.extend_from_slice(&(v.len() as u16).to_be_bytes());
            data.extend_from_slice(v.as_bytes());
        }
        data.push(0);
        let json = payload_to_json(&data).unwrap();
        assert_eq!(json["password"], "abc");
        assert_eq!(json["confirm"], "abc");

        // the same payload behind a presence boolean
        let mut with_flag = vec![1];
        with_flag.extend_from_slice(&data);
        assert_eq!(payload_to_json(&with_flag).unwrap(), json);
    }

    #[test]
    fn empty_or_truncated_payload_is_none() {
        assert!(payload_to_json(&[]).is_none());
        assert!(payload_to_json(&[0]).is_none());
        assert!(payload_to_json(&[10, 8, 0, 5, b'a']).is_none());
    }

    #[test]
    fn oversized_dialog_is_rejected() {
        let body = vec![b' '; MAX_DIALOG_BYTES + 1];
        assert!(dialog_from_json(&body).is_err());
    }
}
