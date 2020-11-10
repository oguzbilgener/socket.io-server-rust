use bytes::Bytes;
use serde_json::Value as JsonValue;
use std::fmt;
use std::{convert::TryFrom, fmt::Debug};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    Connect = 0,
    Disconnect = 1,
    Event = 2,
    Ack = 3,
    ConnectError = 4,
    BinaryEvent = 5,
    BinaryAck = 6,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryPacketType {
    BinaryEvent = 5,
    BinaryAck = 6,
}

impl From<BinaryPacketType> for PacketType {
    fn from(packet_type: BinaryPacketType) -> Self {
        match packet_type {
            BinaryPacketType::BinaryAck => PacketType::BinaryAck,
            BinaryPacketType::BinaryEvent => PacketType::BinaryEvent,
        }
    }
}

/// Kinda like `serde_json`'s Value but also with binary.
pub enum PacketDataValue {
    /// Represents a JSON null value.
    Null,

    /// Represents a JSON boolean.
    Bool(bool),

    /// Represents a JSON number, whether integer or floating point.
    Number(serde_json::Number),

    /// Represents a JSON string.
    String(String),

    /// Represents a JSON array.
    Array(Vec<PacketDataValue>),

    /// Represents a JSON object.
    Object(serde_json::Map<String, PacketDataValue>),

    /// Represents a custom binary object
    Binary(Bytes),
}

impl PartialEq for PacketDataValue {
    fn eq(&self, other: &Self) -> bool {
        todo!()
    }
}

impl Debug for PacketDataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl Clone for PacketDataValue {
    fn clone(&self) -> Self {
        todo!()
    }
}

pub enum ProtoDataValue {
    /// Represents a JSON null value.
    Null,

    /// Represents a JSON boolean.
    Bool(bool),

    /// Represents a JSON number, whether integer or floating point.
    Number(serde_json::Number),

    /// Represents a JSON string.
    String(String),

    /// Represents a JSON array.
    Array(Vec<PacketDataValue>),

    /// Represents a JSON object.
    Object(serde_json::Map<String, PacketDataValue>),

    /// Represents a custom binary object but it's a placeholder
    Binary { placeholder_index: u8 },
}

impl From<JsonValue> for PacketDataValue {
    fn from(_: JsonValue) -> Self {
        todo!()
    }
}

impl TryFrom<ProtoDataValue> for PacketDataValue {
    type Error = &'static str;

    fn try_from(proto: ProtoDataValue) -> Result<Self, Self::Error> {
        Err("unimplemented")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub packet_type: PacketType,
    // this is ack id?
    pub id: Option<u64>,
    pub nsp: String,
    pub data: PacketDataValue,
}

pub struct MultipartPacketHeader {
    packet_type: BinaryPacketType,
    id: Option<u64>,
    nsp: Option<String>,
    data: Option<ProtoDataValue>,
    attachments_count: u8,
}

impl MultipartPacketHeader {
    pub fn has_attachments(&self) -> bool {
        self.attachments_count > 0
    }
}

pub enum ProtoPacket {
    Multipart { header: MultipartPacketHeader },
    Plain { packet: Packet },
}

impl ProtoPacket {
    pub fn has_attachments(&self) -> bool {
        match self {
            ProtoPacket::Multipart { header, .. } => header.has_attachments(),
            ProtoPacket::Plain { .. } => false,
        }
    }

    pub fn packet_type(&self) -> PacketType {
        match self {
            ProtoPacket::Multipart { header } => header.packet_type.to_owned().into(),
            ProtoPacket::Plain { packet } => packet.packet_type,
        }
    }
}

// TODO: make this an enum
pub struct PacketParseError;
