use minecraft_protocol::prelude::{
    BinaryWriter, BinaryWriterError, EncodePacket, Identifiable, ProtocolVersion,
};
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Default)]
pub struct RawPacket {
    data: Vec<u8>,
}

#[derive(Error, Debug)]
pub enum RawPacketError {
    #[error("invalid packet length")]
    InvalidPacketLength,
    #[error("failed to encode packet {id} for version {version}")]
    EncodePacket { id: u8, version: i32 },
}

/// Packet ids are VarInts on the wire. Ids of 128 and above (e.g. `show_dialog`) take two bytes;
/// writing them as a single byte makes the client read the first body byte as part of the id.
fn packet_id_varint(packet_id: u8) -> Vec<u8> {
    if packet_id < 0x80 {
        vec![packet_id]
    } else {
        vec![(packet_id & 0x7F) | 0x80, packet_id >> 7]
    }
}

impl RawPacket {
    /// Creates a raw packet, containing its ID and associated data.
    /// The data vector must not be length padded.
    pub fn new(data: Vec<u8>) -> Result<Self, RawPacketError> {
        if data.is_empty() {
            Err(RawPacketError::InvalidPacketLength)
        } else {
            Ok(Self { data })
        }
    }

    pub fn from_bytes(packet_id: u8, bytes: &[u8]) -> Self {
        let mut data = packet_id_varint(packet_id);
        data.extend_from_slice(bytes);
        Self { data }
    }

    /// Creates a new raw packet from a serializable packet struct.
    pub fn from_packet<T>(
        packet_id: u8,
        version_number: i32,
        packet: &T,
    ) -> Result<Self, BinaryWriterError>
    where
        T: EncodePacket + Identifiable,
    {
        let mut writer = BinaryWriter::new();
        for byte in packet_id_varint(packet_id) {
            writer.write(&byte)?;
        }
        packet.encode(&mut writer, ProtocolVersion::from(version_number))?;

        let data = writer.into_inner();
        Ok(Self { data })
    }

    pub const fn size(&self) -> usize {
        self.data.len()
    }

    pub fn packet_id(&self) -> Option<u8> {
        self.data.first().copied()
    }

    pub fn data(&self) -> &[u8] {
        if self.data.is_empty() {
            &[]
        } else {
            &self.data[1..]
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
}

impl Display for RawPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.data() {
            write!(f, "{byte:02X} ")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod packet_id_tests {
    use super::*;

    #[test]
    fn small_ids_are_one_byte() {
        assert_eq!(packet_id_varint(12), vec![12]);
        assert_eq!(packet_id_varint(127), vec![127]);
    }

    #[test]
    fn large_ids_are_two_byte_varints() {
        assert_eq!(packet_id_varint(140), vec![0x8C, 0x01]);
        assert_eq!(packet_id_varint(143), vec![0x8F, 0x01]);
        assert_eq!(packet_id_varint(255), vec![0xFF, 0x01]);
    }
}
