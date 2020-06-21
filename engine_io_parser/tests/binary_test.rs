use engine_io_parser::binary::decoder;
use engine_io_parser::binary::encoder;
use engine_io_parser::packet::*;

#[test]
fn decodes_packet() {
    let result = decoder::decode_packet(b"\x01asdf abc").unwrap();
    assert_eq!(
        result,
        Packet {
            packet_type: PacketType::Close,
            data: "asdf abc".as_bytes(),
        }
    );
}

#[test]
fn decodes_binary_payload() {
    let result = decoder::decode_payload(b"\x01\x08\xff\x04qqq 123").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0],
        Packet {
            packet_type: PacketType::Message,
            data: decoder::PacketData::BinaryData(b"qqq 123"),
        }
    );
}

#[test]
fn decodes_multi_payload() {
    let result = decoder::decode_payload(
        b"\x01\x09\xff\x04qqq 1235\x01\x07\xff\x02abcd12\x01\x0c\xff\x05abcdq\nwerzx",
    )
    .unwrap();
    assert_eq!(result.len(), 3);
    let packet1 = &result[0];
    let packet2 = &result[1];
    let packet3 = &result[2];
    assert_eq!(packet1.packet_type, PacketType::Message);
    assert_eq!(packet2.packet_type, PacketType::Ping);
    assert_eq!(packet3.packet_type, PacketType::Upgrade);
    assert_eq!(packet1.data, decoder::PacketData::BinaryData(b"qqq 1235"));
    assert_eq!(packet2.data, decoder::PacketData::BinaryData(b"abcd12"));
    assert_eq!(
        packet3.data,
        decoder::PacketData::BinaryData(b"abcdq\nwerzx")
    );
}

#[test]
fn decodes_mixed_payload() {
    assert_eq!(
        decoder::decode_payload(b"\x00\x04\xff\x34\xe2\x82\xac\x01\x05\xff\x04\x01\x02\x03\x04"),
        Ok(vec![
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::PlaintextData("€"),
            },
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::BinaryData(&[1, 2, 3, 4])
            }
        ])
    );
}

#[test]
fn encodes_binary_packet() {
    assert_eq!(
        encoder::encode_packet(Packet {
            packet_type: PacketType::Upgrade,
            data: b"hello world",
        }),
        b"\x05hello world"
    );
    assert_eq!(
        encoder::encode_packet(Packet {
            packet_type: PacketType::Message,
            data: &[16u8, 8u8, 4u8, 2u8],
        }),
        b"\x04\x10\x08\x04\x02"
    );
}

#[test]
fn encodes_binary_payload() {
    assert_eq!(
        encoder::encode_packet(Packet {
            packet_type: PacketType::Message,
            data: &[0x54, 0x32, 0x18, 0x22, 0x44, 0x58],
        }),
        b"\x04\x54\x32\x18\x22\x44\x58"
    )
}

#[test]
fn encodes_plaintext_binary_payload() {
    assert_eq!(
        encoder::encode_payload(&[
            Packet {
                packet_type: PacketType::Message,
                data: encoder::PacketData::PlaintextData("hello"),
            },
            Packet {
                packet_type: PacketType::Ping,
                data: encoder::PacketData::PlaintextData("€")
            }
        ]),
        b"\x00\x06\xff\x34hello\x00\x04\xff\x32\xe2\x82\xac"
    )
}

#[test]
fn encodes_mixed_binary_payload() {
    assert_eq!(
        encoder::encode_payload(&[
            Packet {
                packet_type: PacketType::Message,
                data: encoder::PacketData::PlaintextData("€"),
            },
            Packet {
                packet_type: PacketType::Message,
                data: encoder::PacketData::BinaryData(&[1, 2, 3, 4])
            }
        ]),
        b"\x00\x04\xff\x34\xe2\x82\xac\x01\x05\xff\x04\x01\x02\x03\x04"
    );
}
