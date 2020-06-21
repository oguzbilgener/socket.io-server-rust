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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Packet<T> {
    pub packet_type: PacketType,
    pub data: T,
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
