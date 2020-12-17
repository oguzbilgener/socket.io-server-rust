use bytes::Bytes;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::{convert::TryFrom, fmt::Debug};

static ATTACHMENT_MARKER_KEY: &str = "_placeholder";
static ATTACHMENT_INDEX_KEY: &str = "num";

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

impl PacketType {
    pub fn parse_from_u8<T>(
        value: u8,
        original_input: T,
    ) -> Result<Self, nom::Err<nom::error::Error<T>>> {
        match PacketType::try_from(value) {
            Ok(packet_type) => Ok(packet_type),
            Err(_) => Err(nom::Err::Failure(nom::error::Error::new(
                original_input,
                nom::error::ErrorKind::Digit,
            ))),
        }
    }
}

impl From<BinaryPacketType> for PacketType {
    fn from(packet_type: BinaryPacketType) -> Self {
        match packet_type {
            BinaryPacketType::BinaryAck => PacketType::BinaryAck,
            BinaryPacketType::BinaryEvent => PacketType::BinaryEvent,
        }
    }
}

impl Into<u8> for PacketType {
    fn into(self) -> u8 {
        self as u8
    }
}

// TODO: make this an enum
pub struct PacketParseError;

impl TryFrom<u8> for PacketType {
    type Error = PacketParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PacketType::Connect),
            1 => Ok(PacketType::Disconnect),
            2 => Ok(PacketType::Event),
            3 => Ok(PacketType::Ack),
            4 => Ok(PacketType::ConnectError),
            5 => Ok(PacketType::BinaryEvent),
            6 => Ok(PacketType::BinaryAck),
            _ => Err(PacketParseError),
        }
    }
}

impl TryFrom<PacketType> for BinaryPacketType {
    type Error = ();

    fn try_from(value: PacketType) -> Result<Self, Self::Error> {
        match value {
            PacketType::BinaryAck => Ok(BinaryPacketType::BinaryAck),
            PacketType::BinaryEvent => Ok(BinaryPacketType::BinaryEvent),
            _ => Err(()),
        }
    }
}

/// Like `serde_json`'s Value but also with binary that can be nested deep down
/// as that's something that is allowed in the socket.io spec.
#[derive(Debug, Clone, PartialEq)]
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
    Object(IndexMap<String, PacketDataValue>),

    /// Represents a custom binary object
    /// In JSON, this encodes to: `{ "_placeholder": true, num: i }`
    /// where `i` is the order of the attachment in the attachments trailing the packet
    Binary(Bytes),
}

impl From<JsonValue> for PacketDataValue {
    fn from(value: JsonValue) -> Self {
        match value {
            JsonValue::Null => PacketDataValue::Null,
            JsonValue::Bool(value) => PacketDataValue::Bool(value),
            JsonValue::Number(value) => PacketDataValue::Number(value),
            JsonValue::String(value) => PacketDataValue::String(value),
            JsonValue::Array(array) => PacketDataValue::Array(
                array
                    .into_iter()
                    .map(|el| Into::<PacketDataValue>::into(el))
                    .collect(),
            ),
            JsonValue::Object(obj) => PacketDataValue::Object(obj.into_iter().fold(
                IndexMap::<String, PacketDataValue>::new(),
                |mut map, (k, v)| {
                    map.insert(k, Into::<PacketDataValue>::into(v));
                    map
                },
            )),
        }
    }
}

impl From<Bytes> for PacketDataValue {
    fn from(data: Bytes) -> Self {
        PacketDataValue::Binary(data)
    }
}

impl From<Vec<u8>> for PacketDataValue {
    fn from(data: Vec<u8>) -> Self {
        PacketDataValue::Binary(data.into())
    }
}

impl From<&[u8]> for PacketDataValue {
    fn from(data: &[u8]) -> Self {
        PacketDataValue::Binary(Bytes::copy_from_slice(data))
    }
}

pub(crate) fn get_attachment_index_if_placeholder(
    obj: &serde_json::Map<String, JsonValue>,
) -> Option<usize> {
    let marker = obj.get(ATTACHMENT_MARKER_KEY);
    let index = obj.get(ATTACHMENT_INDEX_KEY);
    if let (Some(marker), Some(index)) = (marker, index) {
        if let JsonValue::Bool(marker) = marker {
            if let JsonValue::Number(index) = index {
                if *marker {
                    return index.as_u64().map(|num| num as usize);
                }
            }
        }
    }
    None
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
    pub packet_type: BinaryPacketType,
    pub id: Option<u64>,
    pub nsp: Option<String>,
    pub data: JsonValue,
    pub attachments_count: u32,
}

impl MultipartPacketHeader {
    pub fn has_attachments(&self) -> bool {
        self.attachments_count > 0
    }
}

pub enum ProtoPacket {
    Multipart(MultipartPacketHeader),
    Plain(Packet),
}

impl ProtoPacket {
    pub fn has_attachments(&self) -> bool {
        match self {
            ProtoPacket::Multipart(header) => header.has_attachments(),
            ProtoPacket::Plain(_) => false,
        }
    }

    pub fn packet_type(&self) -> PacketType {
        match self {
            ProtoPacket::Multipart(header) => header.packet_type.to_owned().into(),
            ProtoPacket::Plain(packet) => packet.packet_type,
        }
    }
}
