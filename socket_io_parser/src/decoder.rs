use core::fmt::Debug;
use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::Display;

use crate::packet::{
    get_attachment_index_if_placeholder, BinaryPacketType, MultipartPacketHeader, Packet,
    PacketDataValue, PacketType, ProtoPacket,
};
use bytes::Bytes;
use nom::{
    bytes::complete::take, bytes::complete::take_till1, character::complete::char as nom_char,
    character::complete::digit1, error::ErrorKind as NomErrorKind, IResult,
};
use std::cell::RefCell;
use std::rc::Rc;

// TODO: these enum variants are mostly unused
#[derive(Debug)]
pub enum HandleEngineMessageError {
    InvalidPacketType,
    PacketParseError,
    InvalidPayload,
    JsonParseError(serde_json::Error),
    TooManyAttachments,
}

impl Display for HandleEngineMessageError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for HandleEngineMessageError {}

impl From<serde_json::Error> for HandleEngineMessageError {
    fn from(err: serde_json::Error) -> Self {
        Self::JsonParseError(err)
    }
}

impl<E> From<nom::Err<E>> for HandleEngineMessageError {
    fn from(_: nom::Err<E>) -> Self {
        Self::PacketParseError
    }
}

pub trait Decoder: 'static + Send + Sync + Sized {
    type HandleError: Debug + Display + From<serde_json::Error> + Sync + Send;

    fn new(packet_header: MultipartPacketHeader) -> Self;

    /// Akin to `decodeString` in `socket.io-parser/lib/index.ts`
    fn decode_proto_packet(text: &str) -> Result<ProtoPacket, Self::HandleError>;
    /// Returning true means all attachments are received, packet is ready to be collected
    fn decode_binary_data(&mut self, data: Bytes) -> Result<bool, Self::HandleError>;
    /// Returns some packet if the packet is ready to collect, `None` otherwise
    fn collect_packet(self) -> Option<Packet>;

    fn make_err() -> Self::HandleError;
}

pub struct DefaultDecoder {
    header: MultipartPacketHeader,
    buffer: Vec<Bytes>,
}

impl DefaultDecoder {
    fn can_collect(&self) -> bool {
        (self.header.attachments_count as usize) == self.buffer.len()
    }

    fn reconstruct_packet_data(
        proto_data: JsonValue,
        attachments: Rc<RefCell<HashMap<usize, Bytes>>>,
    ) -> PacketDataValue {
        match proto_data {
            JsonValue::Null => PacketDataValue::Null,
            JsonValue::Bool(value) => PacketDataValue::Bool(value),
            JsonValue::Number(value) => PacketDataValue::Number(value),
            JsonValue::String(value) => PacketDataValue::String(value),
            JsonValue::Array(array) => PacketDataValue::Array(
                array
                    .into_iter()
                    .map(|el| Self::reconstruct_packet_data(el, attachments.clone()))
                    .collect(),
            ),
            JsonValue::Object(obj) => match get_attachment_index_if_placeholder(&obj) {
                Some(index) => {
                    if let Some(data) = attachments.borrow_mut().remove(&index) {
                        PacketDataValue::Binary(data)
                    } else {
                        PacketDataValue::Null
                    }
                }
                None => PacketDataValue::Object(obj.into_iter().fold(
                    IndexMap::<String, PacketDataValue>::new(),
                    |mut map, (k, v)| {
                        map.insert(k, Self::reconstruct_packet_data(v, attachments.clone()));
                        map
                    },
                )),
            },
        }
    }
}

impl Decoder for DefaultDecoder {
    type HandleError = HandleEngineMessageError;

    fn new(packet_header: MultipartPacketHeader) -> Self {
        DefaultDecoder {
            header: packet_header,
            buffer: Vec::new(),
        }
    }

    fn decode_binary_data(&mut self, data: Bytes) -> Result<bool, Self::HandleError> {
        if self.can_collect() {
            return Err(HandleEngineMessageError::TooManyAttachments);
        }
        self.buffer.push(data);
        Ok(self.can_collect())
    }

    fn decode_proto_packet(text: &str) -> Result<ProtoPacket, Self::HandleError> {
        // TODO: how to get more detailed parse errors from nom?
        match parse_proto_packet(text) {
            Ok((_, packet)) => Ok(packet),
            Err(_err) => Err(HandleEngineMessageError::PacketParseError),
        }
    }

    fn collect_packet(self) -> Option<Packet> {
        if self.can_collect() {
            let map = self.buffer.into_iter().enumerate().fold(
                HashMap::<usize, Bytes>::new(),
                |mut acc, (index, data)| {
                    acc.insert(index, data);
                    acc
                },
            );
            let packet_data =
                Self::reconstruct_packet_data(self.header.data, Rc::new(RefCell::new(map)));
            Some(Packet {
                packet_type: self.header.packet_type.into(),
                id: self.header.id,
                nsp: self.header.nsp.unwrap_or("/".to_owned()),
                data: packet_data,
            })
        } else {
            None
        }
    }

    // TODO: make this more meaningful!
    fn make_err() -> Self::HandleError {
        HandleEngineMessageError::InvalidPayload
    }
}

fn parse_proto_packet(input: &str) -> IResult<&str, ProtoPacket> {
    let (input, packet_type_index) = take_u8(input)?;
    let packet_type = PacketType::parse_from_u8(packet_type_index, input)?;

    // look for attachments count if binary
    let (input, attachments_count) =
        if let Ok(_) = TryInto::<BinaryPacketType>::try_into(packet_type) {
            parse_attachments_count(input)?
        } else {
            (input, 0u32)
        };

    // look up namespace (if any)
    let (input, namespace) = parse_namespace(input)?;

    // look up id (if available)
    let (input, id) = parse_id(input)?;

    // look up json data
    let (input, payload) = parse_json_data(input, packet_type)?;

    if attachments_count > 0 {
        Ok((
            input,
            ProtoPacket::Multipart(MultipartPacketHeader {
                packet_type: packet_type.try_into().unwrap(),
                id,
                nsp: Some(namespace.to_owned()),
                data: payload,
                attachments_count,
            }),
        ))
    } else {
        Ok((
            input,
            ProtoPacket::Plain(Packet {
                packet_type,
                id,
                nsp: namespace.to_owned(),
                data: payload.try_into().map_err(|_| {
                    nom::Err::Failure(nom::error::Error::new(input, NomErrorKind::Digit))
                })?,
            }),
        ))
    }
}

fn parse_attachments_count(input: &str) -> IResult<&str, u32> {
    // take all the digits
    let (input, digits) = digit1(input)?;
    // take the `-` separator
    let (input, _) = nom_char('-')(input)?;
    // convert little-endian digits to u32
    let count = digits_to_u32(digits)
        .map_err(|_| nom::Err::Failure(nom::error::Error::new(input, NomErrorKind::Digit)))?;
    Ok((input, count))
}

fn parse_namespace(input: &str) -> IResult<&str, &str> {
    let orig_input = input;
    // check if the first character is a `/`
    match nom_char::<&str, nom::error::Error<&str>>('/')(input) {
        Ok((_, _)) => {
            let (input, namespace) = take_till1(|c| c == ',')(orig_input)?;
            // consume that trailing comma
            let (input, _) = take(1usize)(input)?;
            Ok((input, namespace))
        }
        Err(_) => Ok((input, "/")),
    }
}

fn parse_id(input: &str) -> IResult<&str, Option<u64>> {
    // take all the digits, if there are any
    match digit1::<&str, nom::error::Error<&str>>(input) {
        Ok((input, digits)) => {
            // take the `-` separator
            let (input, _) = nom_char('-')(input)?;
            // convert little-endian digits to u32
            let id = digits_to_u64(digits).map_err(|_| {
                nom::Err::Failure(nom::error::Error::new(input, NomErrorKind::Digit))
            })?;
            Ok((input, Some(id)))
        }
        // No id in this packet
        Err(_) => return Ok((input, None)),
    }
}

fn parse_json_data(input: &str, packet_type: PacketType) -> IResult<&str, JsonValue> {
    let payload: JsonValue = match serde_json::from_str::<JsonValue>(input) {
        Ok(payload) => payload.into(),
        Err(_) => JsonValue::Null,
    };
    if is_payload_valid(packet_type, &payload) {
        Ok((input, payload))
    } else {
        Err(nom::Err::Failure(nom::error::Error::new(
            input,
            NomErrorKind::Satisfy,
        )))
    }
}

fn is_payload_valid(packet_type: PacketType, payload: &JsonValue) -> bool {
    match packet_type {
        PacketType::Connect => match payload {
            JsonValue::Object(_) => true,
            _ => false,
        },
        PacketType::Disconnect => match payload {
            JsonValue::Null => true,
            _ => false,
        },
        PacketType::ConnectError => match payload {
            JsonValue::String(_) | JsonValue::Object(_) => true,
            _ => false,
        },
        PacketType::Event | PacketType::BinaryEvent => match payload {
            JsonValue::Array(vec) => match vec.get(0) {
                Some(JsonValue::String(_)) => true,
                _ => false,
            },
            _ => false,
        },
        PacketType::Ack | PacketType::BinaryAck => match payload {
            JsonValue::Array(_) => true,
            _ => false,
        },
    }
}

fn take_u8(input: &str) -> IResult<&str, u8> {
    let (input, num) = digit1(input)?;
    if let Ok(num) = num.parse::<u8>() {
        Ok((input, num))
    } else {
        Err(nom::Err::Failure(nom::error::Error::new(
            input,
            NomErrorKind::Digit,
        )))
    }
}

fn digits_to_u32(digits: &str) -> Result<u32, ()> {
    let digit_count = digits.chars().count() as u32;
    digits.chars().enumerate().try_fold(0, |acc, (order, ch)| {
        Ok(acc + ch.to_digit(10).ok_or(())? * (10u32).pow(digit_count - order as u32))
    })
}

fn digits_to_u64(digits: &str) -> Result<u64, ()> {
    digits_to_u32(digits).map(|num| num as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_u8() {
        assert_eq!(take_u8("0"), Ok(("", 0u8)));
        assert_eq!(take_u8("254"), Ok(("", 254u8)));
        assert_eq!(
            take_u8("hello"),
            Err(nom::Err::Error(nom::error::Error::new(
                "hello",
                nom::error::ErrorKind::Digit
            )))
        );
    }
}
