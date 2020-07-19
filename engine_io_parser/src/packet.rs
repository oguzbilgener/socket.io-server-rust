use std::borrow::Cow;
use std::convert::TryFrom;
use std::error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PacketType {
    Open = 0,
    Close = 1,
    Ping = 2,
    Pong = 3,
    Message = 4,
    Upgrade = 5,
    Noop = 6,
}

impl Into<u8> for PacketType {
    fn into(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for PacketType {
    type Error = ParsePacketError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PacketType::Open),
            1 => Ok(PacketType::Close),
            2 => Ok(PacketType::Ping),
            3 => Ok(PacketType::Pong),
            4 => Ok(PacketType::Message),
            5 => Ok(PacketType::Upgrade),
            6 => Ok(PacketType::Noop),
            _ => Err(ParsePacketError {
                message: "Invalid packet".to_owned(),
            }),
        }
    }
}

impl PacketType {
    pub fn parse_from_u8<T>(
        value: u8,
        original_input: T,
    ) -> Result<Self, nom::Err<(T, nom::error::ErrorKind)>> {
        match PacketType::try_from(value) {
            Ok(packet_type) => Ok(packet_type),
            Err(_) => Err(nom::Err::Failure((
                original_input,
                nom::error::ErrorKind::Digit,
            ))),
        }
    }
}

/// Use this enum to figure out if a packet in a payload has binary or string data.
/// In string-encoded payloads, each packet may contain string or binary data.
/// Binary data in string-encoded payloads is encoded to base64. Otherwise, string
/// data is returned as is.
/// In binary-encoded payloads, each packet may contain string or binary data.
/// If a packet in a payload has string data, it's parsed as a UTF-8 string.
#[derive(Debug, Clone, PartialEq)]
pub enum PacketData<'a> {
    Plaintext(Cow<'a, str>),
    Binary(Cow<'a, [u8]>),
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Packet<'a> {
    pub packet_type: PacketType,
    pub data: PacketData<'a>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsePacketError {
    pub message: String,
}

impl error::Error for ParsePacketError {}

impl fmt::Display for ParsePacketError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl<'a> From<&'a str> for PacketData<'a> {
    fn from(val: &'a str) -> Self {
        PacketData::Plaintext(Cow::Borrowed(val))
    }
}

impl<'a> From<&'a [u8]> for PacketData<'a> {
    fn from(val: &'a [u8]) -> Self {
        PacketData::Binary(Cow::Borrowed(val))
    }
}

impl<'a> From<Vec<u8>> for PacketData<'a> {
    fn from(val: Vec<u8>) -> Self {
        PacketData::Binary(Cow::Owned(val))
    }
}
