use core::fmt::Debug;
use std::fmt::Display;
use engine_io_parser::packet::Packet as EnginePacket;
use engine_io_parser::packet::PacketData as EnginePacketData;
use engine_io_parser::packet::PacketType as EnginePacketType;
use engine_io_parser::packet::ParsePacketError as EngineParserPacketError;

use crate::packet::{Packet, ProtoPacket, MultipartPacketHeader};
use bytes::Bytes;


// TODO: come up with a better error type, maybe make it an enum
#[derive(Debug)]
pub struct HandleEngineMessageError {
    reason: &'static str,
}

impl Display for HandleEngineMessageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

pub trait Decoder: 'static + Send + Sync + Sized {

    type HandleError: Debug + Display;

    fn new(packet_header: MultipartPacketHeader) -> Self;

    fn decode_binary_data(&mut self, data: Bytes) -> Result<bool, Self::HandleError>;
    fn decode_proto_packet(text: String) -> Result<ProtoPacket, Self::HandleError>;
    fn collect_packet(&mut self) -> Packet;
}

pub struct DefaultDecoder {
    buffer: Vec<EnginePacket>,
}

impl Decoder for DefaultDecoder {

    type HandleError = HandleEngineMessageError;

    fn new(packet_header: MultipartPacketHeader) -> Self {
        DefaultDecoder {
            buffer: Vec::new()
        }
    }

    fn decode_binary_data(&mut self, data: Bytes) -> Result<bool, Self::HandleError> {
        todo!();
    }
    fn decode_proto_packet(text: String) -> Result<ProtoPacket, Self::HandleError> {
        todo!();
    }
    fn collect_packet(&mut self) -> Packet {
        todo!();
    }
}