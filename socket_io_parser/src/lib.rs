use bytes::Bytes;
use serde_json::Value as JsonValue;

// TODO: revisit this file once we implement the rest of socket.io server
// So that we have all the requirements and can find the best performing solution

pub mod engine_io_parser {
    pub use engine_io_parser::packet::PacketData as EnginePacketData;
    pub use engine_io_parser::packet::Packet as EnginePacket;
    pub use engine_io_parser::packet::PacketType as EnginePacketType;
    pub use engine_io_parser::packet::ParsePacketError as EngineParserPacketError;
}

pub mod packet;
pub mod parser;
pub mod decoder;
pub mod encoder;

pub use crate::packet::*;
pub use crate::parser::*;

pub use crate::parser::Parser;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
