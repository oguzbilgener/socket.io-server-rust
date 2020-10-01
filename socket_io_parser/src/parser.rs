use bytes::Bytes;
use serde_json::Value as JsonValue;

use crate::packet::Packet;
use crate::packet::PacketParseError;
use crate::packet::ProtoPacket;
use crate::decoder::{Decoder, DefaultDecoder};

pub trait Parser {
    type Encoder;
    type Decoder: 'static + Decoder + Send + Sync;
    // fn encode(packet: Packet) -> Encoded;
    // fn decode_text(text: String) -> Result<ProtoPacket, PacketParseError>;
}

pub struct Encoded {
    core: JsonValue,
    attachments: Option<Vec<Bytes>>,
}

fn make_placeholder() -> JsonValue {
    unimplemented!();
}

fn encode_packet(packet: Packet) -> Encoded {
    unimplemented!();
    /*
    let packet_type = packet.packet_type as u8;
    if packet.packet_type == PacketType::BinaryEvent || packet.packet_type == PacketType::BinaryAck
    {
        // TODO: append attachments count
        // TODO: append '-'
    }
    if let Some(nsp) = packet.nsp {
        if nsp != "/" {
            // TODO: append `nsp`
            // TODO: append: ','
        }
    }
    if let Some(id) = packet.id {
        // TODO: append `id`
    }
    if let Some(data) = packet.data {
        let serialized = serde_json::to_string(&data);
        // TODO: append `serialized`
    }
    // TODO: return it
    String::new()
    */
}


pub struct DefaultParser;

impl Parser for DefaultParser {

    // TODO:fix this
    type Encoder = Bytes;
    type Decoder = DefaultDecoder;

}