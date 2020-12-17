// TODO: revisit this file once we implement the rest of socket.io server
// So that we have all the requirements and can find the best performing solution

pub mod engine_io_parser {
    pub use engine_io_parser::packet::Packet as EnginePacket;
    pub use engine_io_parser::packet::PacketData as EnginePacketData;
    pub use engine_io_parser::packet::PacketType as EnginePacketType;
    pub use engine_io_parser::packet::ParsePacketError as EngineParserPacketError;
}

#[macro_use]
mod macros;
mod ser;
pub mod decoder;
pub mod encoder;
pub mod packet;

pub use crate::ser::Serializer;
pub use crate::packet::*;

pub trait Parser {
    type Encoder: 'static + encoder::Encoder + Send + Sync;
    type Decoder: 'static + decoder::Decoder + Send + Sync;
}

pub struct DefaultParser;

impl Parser for DefaultParser {
    type Encoder = encoder::DefaultEncoder;
    type Decoder = decoder::DefaultDecoder;
}

pub fn to_value<T>(value: T) -> Result<PacketDataValue, ser::Error>
where
    T: serde::ser::Serialize,
{
    value
        .serialize(ser::Serializer)
        .map(|value| value)
}

pub use indexmap::IndexMap as Map;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
