use crate::binary::decoder as binary_decoder;
use crate::string::decoder as string_decoder;
use crate::packet::{Packet, ParsePacketError};

/// Represents the format of the whole encoded packet
#[derive(Debug, Clone, PartialEq)]
pub enum Encoded<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}

pub fn decode_packet(input: Encoded) -> Result<Packet, ParsePacketError> {
    match input {
        Encoded::Text(text) => string_decoder::decode_packet(text),
        Encoded::Binary(binary) => binary_decoder::decode_packet(binary),
    }
}
