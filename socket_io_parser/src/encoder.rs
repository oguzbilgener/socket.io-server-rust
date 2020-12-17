use bytes::Bytes;
use engine_io_parser::packet::{
    Packet as EnginePacket, PacketData as EnginePacketData, PacketType as EnginePacketType,
};
use serde_json::{json, Value as JsonValue};
use std::cell::RefCell;
use std::rc::Rc;

use crate::packet::{Packet, PacketDataValue, PacketType};

pub trait Encoder: 'static + Send + Sync + Sized {
    fn encode_packet(packet: Packet) -> EncodedPacket;
}

pub struct DefaultEncoder;

pub struct EncodedPacket {
    /// JSON encoded to String
    pub packet: String,
    /// Optional list of binary attachments that cannot be sent in JSON
    pub attachments: Vec<Bytes>,
}

impl From<EncodedPacket> for Vec<EnginePacket> {
    fn from(encoded: EncodedPacket) -> Self {
        let header_packet = EnginePacket {
            packet_type: EnginePacketType::Message,
            data: EnginePacketData::Text(encoded.packet),
        };
        let mut packets = vec![header_packet];
        packets.append(
            &mut encoded
                .attachments
                .into_iter()
                .map(|attachment| EnginePacket {
                    packet_type: EnginePacketType::Message,
                    data: EnginePacketData::Binary(attachment),
                })
                .collect::<Vec<EnginePacket>>(),
        );
        packets
    }
}

impl DefaultEncoder {
    fn deconstruct_packet_data(
        packet_data: PacketDataValue,
        attachments: Rc<RefCell<Vec<Bytes>>>,
    ) -> JsonValue {
        match packet_data {
            PacketDataValue::Null => JsonValue::Null,
            PacketDataValue::Bool(value) => JsonValue::Bool(value),
            PacketDataValue::Number(value) => JsonValue::Number(value),
            PacketDataValue::String(value) => JsonValue::String(value),
            PacketDataValue::Array(array) => JsonValue::Array(
                array
                    .into_iter()
                    .map(|el| Self::deconstruct_packet_data(el, attachments.clone()))
                    .collect(),
            ),
            PacketDataValue::Object(obj) => obj
                .into_iter()
                .map(|(_, v)| Self::deconstruct_packet_data(v, attachments.clone()))
                .collect(),
            PacketDataValue::Binary(data) => {
                let map = make_placeholder(attachments.borrow().len());
                attachments.borrow_mut().push(data);
                map
            }
        }
    }
}

impl Encoder for DefaultEncoder {
    fn encode_packet(packet: Packet) -> EncodedPacket {
        let packet_type = (packet.packet_type as u8).to_string();
        let attachments = Rc::new(RefCell::new(Vec::new()));
        let header = Self::deconstruct_packet_data(packet.data, attachments.clone());
        let attachments = attachments.replace(Vec::new());
        let attachments_count = attachments.len().to_string();

        // TODO: start the string with the type
        let mut buffer: Vec<&str> = Vec::new();
        buffer.push(&packet_type);

        if packet.packet_type == PacketType::BinaryEvent
            || packet.packet_type == PacketType::BinaryAck
        {
            buffer.push(&attachments_count);
            buffer.push("-");
        }
        if packet.nsp != "/" {
            buffer.push(&packet.nsp);
            buffer.push(",");
        }
        let id = packet.id.map(|id| id.to_string());
        if let Some(id) = &id {
            buffer.push(id);
        }

        let serialized = if header != JsonValue::Null {
            // This should not fail with an error since the keys are always string in JsonValue and PacketDataValue.
            Some(serde_json::to_string(&header).expect("Failed to serialize header packet"))
        } else {
            None
        };
        if let Some(serialized) = &serialized {
            buffer.push(serialized);
        }

        let encoded_packet = (&buffer).join("");

        EncodedPacket {
            packet: encoded_packet,
            attachments,
        }
    }
}

fn make_placeholder(index: usize) -> JsonValue {
    json!({
        "_placeholder": true,
        "num": index as u64
    })
}
