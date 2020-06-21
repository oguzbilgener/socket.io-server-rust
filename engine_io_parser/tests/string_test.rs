use engine_io_parser::packet::*;
use engine_io_parser::string::decoder;
use engine_io_parser::string::encoder;

#[test]
fn decodes_string_packet() {
    let res = decoder::decode_packet("5asdf").unwrap();
    assert_eq!(
        res,
        Packet {
            packet_type: PacketType::Upgrade,
            data: "asdf"
        }
    );
}

#[test]
fn decodes_unicode_string_packet() {
    let res = decoder::decode_packet("2hello world 🦀  안녕 잘 지내?").unwrap();
    assert_eq!(
        res,
        Packet {
            packet_type: PacketType::Ping,
            data: "hello world 🦀  안녕 잘 지내?"
        }
    );
}

#[test]
fn decodes_plaintext_string_payload() {
    let res = decoder::decode_payload("6:4hello2:4€").unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(
        res,
        vec![
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::PlaintextData("hello"),
            },
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::PlaintextData("€")
            }
        ],
    );
}

#[test]
fn decodes_binary_string_payload() {
    let res = decoder::decode_payload("10:b4aGVsbG8=10:b4AQIDBA==").unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(
        res,
        vec![
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::BinaryData(vec![b'h', b'e', b'l', b'l', b'o']),
            },
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::BinaryData(vec![1, 2, 3, 4])
            }
        ],
    );
}

#[test]
fn decodes_mixed_string_payload() {
    let res = decoder::decode_payload("2:4€10:b4AQIDBA==").unwrap();
    assert_eq!(res.len(), 2);
    assert_eq!(
        res,
        vec![
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::PlaintextData("€"),
            },
            Packet {
                packet_type: PacketType::Message,
                data: decoder::PacketData::BinaryData(vec![1, 2, 3, 4])
            }
        ],
    );
}

#[test]
fn encodes_string_packet() {
    let packet = Packet {
        packet_type: PacketType::Message,
        data: "Hello world!",
    };
    assert_eq!(encoder::encode_packet(packet), "4Hello world!");
}

#[test]
fn encodes_plaintext_string_payload() {
    assert_eq!(
        encoder::encode_payload(&[
            Packet {
                packet_type: PacketType::Message,
                data: encoder::PacketData::PlaintextData("hello"),
            },
            Packet {
                packet_type: PacketType::Message,
                data: encoder::PacketData::PlaintextData("€")
            }
        ]),
        "6:4hello2:4€"
    );
}

#[test]
fn encodes_mixed_string_payload() {
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
        "2:4€10:b4AQIDBA=="
    );
}

#[test]
fn end_to_end_string_packet_decode() {
    let encoded = "2hello world 🦀  안녕 잘 지내?";
    assert_eq!(
        encoder::encode_packet(decoder::decode_packet(encoded).unwrap()),
        encoded
    );
}

#[test]
fn end_to_end_string_packet_encode() {
    let packet = Packet {
        packet_type: PacketType::Ping,
        data: "hello world 🦀  안녕 잘 지내?",
    };
    assert_eq!(
        decoder::decode_packet(&encoder::encode_packet(packet)).unwrap(),
        packet
    );
}
