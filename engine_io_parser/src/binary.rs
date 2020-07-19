use crate::packet::{Packet, PacketData, PacketType, ParsePacketError};
use nom::{
    bytes::complete::take, bytes::complete::take_till1, character::complete::digit1,
    error::ErrorKind, multi::fold_many0, number::complete::le_u8, Err::Failure, IResult,
};
use std::borrow::Cow;

pub mod decoder {
    use super::*;

    /// Decode a packet with binary data from a u8 slice.
    /// Each packet has a packet type (Open, Close, Message...) and a data section.
    /// `decode_packet` assumes the packet's data is meant to be binary.
    ///
    /// # Arguments
    /// * `input` - A slice containing the binary-encoded packet
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::binary::decoder::*;
    ///
    /// assert_eq!(decode_packet(&[4u8, 1u8, 2u8, 3u8, 4u8]), Ok(Packet {
    ///     packet_type: PacketType::Message,
    ///     data: vec![1, 2, 3, 4].into(),
    /// }))
    /// ```
    pub fn decode_packet(input: &[u8]) -> Result<Packet, ParsePacketError> {
        if let Ok((_, packet)) = parse_binary_data_packet(input, None) {
            Ok(packet)
        } else {
            Err(ParsePacketError {
                message: "Unable to decode packet".to_owned(),
            })
        }
    }

    /// Decode a binary-encoded payload containing multiple packets.
    /// Returns a Vec of parsed `Packet`s, with the appropriate `PacketData` type
    /// for each packet if successful.
    /// In an encoded payload, each packet is denoted with its length and its
    /// data type; either binary or string. This function returns binary packet
    /// data as is, and parses string packets to UTF-8.
    ///
    /// # Arguments
    /// * `input` - A slice containing the binary-encoded payload
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::binary::decoder::*;
    ///
    /// assert_eq!(
    ///     decode_payload(b"\x00\x04\xff\x34\xe2\x82\xac\x01\x05\xff\x04\x01\x02\x03\x04"),
    ///     Ok(vec![
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: "€".into(),
    ///         },
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: vec![1u8, 2u8, 3u8, 4u8].into()
    ///         }
    ///     ])
    /// );
    /// ```
    pub fn decode_payload(input: &[u8]) -> Result<Vec<Packet>, ParsePacketError> {
        if let Ok((_, packets)) = parse_binary_payload(input) {
            Ok(packets)
        } else {
            Err(ParsePacketError {
                message: "Unable to decode payload".to_owned(),
            })
        }
    }
}

pub mod encoder {
    use super::*;

    /// Encode a packet with binary data to a byte array.
    ///
    /// # Arguments
    /// * `input` - A `packet` with binary data
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::binary::encoder::*;
    ///
    /// assert_eq!(
    ///     encode_packet(&Packet {
    ///         packet_type: PacketType::Message,
    ///         data: vec![16u8, 8u8, 4u8, 2u8].into(),
    ///     }),
    ///     b"\x04\x10\x08\x04\x02"
    /// );
    /// ```
    pub fn encode_packet(input: &Packet) -> Vec<u8> {
        serialize_packet(input.packet_type, &input.data)
    }

    /// Encode a payload containing multiple packets with either binary or string
    /// data to a byte array.
    /// In an encoded payload, each packet is denoted with its length and its
    /// data type; either binary or string. This function passes binary packet
    /// data as is, and encodes string packets to bytes.
    ///
    /// # Arguments
    /// * `input`  - A list of `Packet`s with either string or binary data each
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::binary::encoder::*;
    ///
    /// assert_eq!(
    ///    encode_payload(&[
    ///        Packet {
    ///            packet_type: PacketType::Message,
    ///            data: "€".into(),
    ///        },
    ///        Packet {
    ///            packet_type: PacketType::Message,
    ///            data: vec![1u8, 2u8, 3u8, 4u8].into()
    ///        }
    ///    ]),
    ///    b"\x00\x04\xff\x34\xe2\x82\xac\x01\x05\xff\x04\x01\x02\x03\x04"
    ///);
    /// ```
    pub fn encode_payload<'a>(input: &'a [Packet]) -> Vec<u8> {
        serialize_payload(input)
    }
}

fn parse_binary_data_packet(input: &[u8], data_length: Option<usize>) -> IResult<&[u8], Packet> {
    // In binary data  packets, the packet type is encoded in binary
    let (input, packet_type_index) = le_u8(input)?;
    let take_amount = match data_length {
        Some(l) => l,
        None => input.len(),
    };
    let (input, data) = take(take_amount)(input)?;
    let packet_type = PacketType::parse_from_u8(packet_type_index, input)?;
    Ok((
        input,
        Packet {
            packet_type,
            data: PacketData::Binary(Cow::Borrowed(data)),
        },
    ))
}

fn parse_string_data_packet(input: &[u8], data_length: usize) -> IResult<&[u8], Packet> {
    // In string data packets, the packet type is encoded in ASCII
    let (input, packet_type_ascii) = digit1(input)?;
    let (input, data) = take(data_length)(input)?;
    let packet_type = PacketType::parse_from_u8(packet_type_ascii[0] - b'0', input)?;
    match std::str::from_utf8(data) {
        Ok(parsed_str) => Ok((
            input,
            Packet {
                packet_type,
                data: PacketData::Plaintext(Cow::Borrowed(parsed_str)),
            },
        )),
        Err(_) => Err(nom::Err::Failure((
            input,
            nom::error::ErrorKind::AlphaNumeric,
        ))),
    }
}

fn parse_binary_payload(input: &[u8]) -> IResult<&[u8], Vec<Packet>> {
    fold_many0(
        parse_binary_packet_in_payload,
        Vec::new(),
        |mut acc: Vec<Packet>, item| {
            acc.push(item);
            acc
        },
    )(input)
}

fn parse_binary_packet_in_payload(input: &[u8]) -> IResult<&[u8], Packet> {
    let (input, packet_data_type) = parse_packet_data_type_in_binary_payload(input)?;
    // binary packet length in a payload is the actual packet data length + 1 (packet type)
    let (input, packet_length) = parse_packet_length(input)?;
    // take the white space which is the binary separator (already validated)
    let (input, _) = take(1u8)(input)?;
    if packet_data_type == 0u8 {
        // String data. Try to parse the data as an utf8 string just like the original JS library does.
        let (input, packet) = parse_string_data_packet(input, (packet_length - 1) as usize)?;
        Ok((input, packet))
    } else {
        // packet length also contains the packet type, but we're sending the length of the data part.
        let (input, packet) = parse_binary_data_packet(input, Some((packet_length - 1) as usize))?;
        Ok((input, packet))
    }
}

fn parse_packet_length(input: &[u8]) -> IResult<&[u8], usize> {
    // packet length is in bytes until the `ff` separator
    let (input, packet_length_encoded) = take_till1(|c| c == 255u8)(input)?;
    // Looking at the decodePayloadAsBinary of `engine.io-parser` JS library,
    // each byte here is a digit of the base-10 integer (facepalm)
    let mut i = packet_length_encoded.len();
    let mut packet_length = 0;
    if i == 0 {
        return Ok((input, packet_length_encoded[0] as usize));
    }
    while i > 0 {
        packet_length += (10u32.pow((i - 1) as u32)) * (packet_length_encoded[i - 1] as u32);
        i -= 1;
    }
    Ok((input, packet_length as usize))
}

fn encode_packet_length(input: usize) -> Vec<u8> {
    let mut val = input;
    let mut digits: Vec<u8> = Vec::new();
    if val == 0 {
        digits.push(0u8);
        return digits;
    }
    while val > 0 {
        let digit = val % 10;
        val /= 10;
        digits.push(digit as u8);
    }
    digits
}

fn parse_packet_data_type_in_binary_payload(input: &[u8]) -> IResult<&[u8], u8> {
    // first byte is either 0 (string) or 1 (byte).
    let (input, packet_type_arr) = take(1u8)(input)?;
    let packet_type = packet_type_arr[0];
    if packet_type == 0u8 || packet_type == 1u8 {
        Ok((input, packet_type))
    } else {
        Err(Failure((input, ErrorKind::Digit)))
    }
}

fn serialize_packet<'a>(packet_type: PacketType, data: &'a PacketData) -> Vec<u8> {
    let packet_type_u8 = packet_type as u8;
    match data {
        PacketData::Plaintext(data) => {
            // UTF-8 string to bytes conversion
            let data = data.as_bytes();
            // + 1 for the packet type
            let mut sequence = Vec::with_capacity(data.len() + 1);
            // plaintext packet type is encoded as an ASCII number
            sequence.push(packet_type_u8 + b'0');
            sequence.extend_from_slice(data);
            sequence
        }
        PacketData::Binary(data) => {
            let mut sequence = Vec::with_capacity(data.len() + 1);
            sequence.push(packet_type_u8);
            sequence.extend_from_slice(data);
            sequence
        }
        PacketData::Empty => vec![packet_type_u8 + b'0'],
    }
}

fn serialize_packet_in_payload<'a>(packet_type: PacketType, data: &'a PacketData) -> Vec<u8> {
    let packet_type_u8 = packet_type as u8;
    match data {
        PacketData::Plaintext(data) => {
            // + 1 for the packet type
            let length_bytes = encode_packet_length(data.len() + 1);
            // UTF-8 string to bytes conversion
            let data = data.as_bytes();
            let mut sequence = Vec::with_capacity(length_bytes.len() + data.len() + 3);
            sequence.push(0u8);
            let mut sequence = [sequence, length_bytes].concat();
            sequence.push(255u8);
            // plaintext packet type is encoded as an ASCII number
            sequence.push(packet_type_u8 + b'0');
            sequence.extend_from_slice(data);
            sequence
        }
        PacketData::Binary(data) => {
            // + 1 for the packet type
            let length_bytes = encode_packet_length(data.len() + 1);
            let mut sequence = Vec::with_capacity(length_bytes.len() + data.len() + 3);
            sequence.push(1u8);
            let mut sequence = [sequence, length_bytes].concat();
            sequence.push(255u8);
            sequence.push(packet_type_u8);
            sequence.extend_from_slice(data);
            sequence
        }
        PacketData::Empty => vec![1u8, b'0', 255u8, packet_type_u8 + b'0'],
    }
}

fn serialize_payload<'a>(packets: &'a [Packet]) -> Vec<u8> {
    packets.iter().fold(Vec::new(), |acc, packet| {
        let serialized = serialize_packet_in_payload(packet.packet_type, &packet.data);
        [acc, serialized].concat()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::error::ErrorKind;
    use nom::Err::Error;

    #[test]
    fn test_encode_packet_length() {
        assert_eq!(encode_packet_length(0), &[0]);
        assert_eq!(encode_packet_length(1), &[1]);
        assert_eq!(encode_packet_length(5), &[5]);
        assert_eq!(encode_packet_length(10), &[0, 1]);
        assert_eq!(encode_packet_length(42), &[2, 4]);
        assert_eq!(encode_packet_length(190), &[0, 9, 1]);
        assert_eq!(encode_packet_length(190), &[0, 9, 1]);
        assert_eq!(encode_packet_length(8214), &[4, 1, 2, 8]);
        assert_eq!(encode_packet_length(918214), &[4, 1, 2, 8, 1, 9]);
    }

    #[test]
    fn test_parse_packet_length() {
        let input = [4u8, 255u8];
        assert_eq!(parse_packet_length(&input[..]), Ok((&input[1..], 4)));
        let input = [0u8, 2u8, 3u8, 255u8];
        assert_eq!(parse_packet_length(&input[..]), Ok((&input[3..], 320)));
        let input = [3u8, 0u8, 0u8, 255u8];
        assert_eq!(parse_packet_length(&input[..]), Ok((&input[3..], 3)));
        let input = [9u8, 9u8, 4u8, 2u8, 5u8, 255u8];
        assert_eq!(parse_packet_length(&input[..]), Ok((&input[5..], 52499)));
        let input: Vec<u8> = Vec::new();
        assert_eq!(
            Error((&input[..], ErrorKind::TakeTill1)),
            parse_packet_length(&input[..]).unwrap_err()
        );
    }
}
