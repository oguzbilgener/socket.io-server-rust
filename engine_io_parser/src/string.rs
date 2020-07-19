use crate::packet::{Packet, PacketData, PacketType, ParsePacketError};
use nom::{
    branch::alt, bytes::complete::take, character::complete::char as nom_char,
    character::complete::digit1, multi::fold_many0, Err::Failure, IResult,
};
use std::borrow::Cow;

pub mod decoder {
    use super::*;

    /// Decode a packet with string data from a UTF-8 string.
    /// Each packet has a packet type (Open, Close, Message...) and a data section.
    /// `decode_packet` assumes the packet's data is a string.
    ///
    /// # Arguments
    /// * `input` - A slice containing the string-encoded packet
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketType};
    /// use engine_io_parser::string::decoder::*;
    ///
    /// assert_eq!(decode_packet("4Hello world!"), Ok(Packet {
    ///     packet_type: PacketType::Message,
    ///     data: "Hello world!".into(),
    /// }))
    /// ```
    pub fn decode_packet(input: &str) -> Result<Packet, ParsePacketError> {
        if let Ok((_, packet)) = parse_string_packet(input, None) {
            Ok(packet)
        } else {
            Err(ParsePacketError {
                message: "Unable to decode packet".to_owned(),
            })
        }
    }

    /// Decode a string-encoded payload containing multiple packets.
    /// Returns a Vec of parsed `Packet`s, with the appropriate `PacketData` type
    /// for each packet if successful.
    /// In an encoded payload, each packet is denoted with its length and its
    /// data type; either binary or string. This function returns string packet
    /// data as is, and parses binary packets from base64 to a byte array.
    ///
    /// # Arguments
    /// * `input` - a string containing an encoded payload.
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::string::decoder::*;
    ///
    /// assert_eq!(
    ///     decode_payload("6:4hello2:4€"),
    ///     Ok(vec![
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: "hello".into(),
    ///         },
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: "€".into()
    ///         }
    ///     ]),
    /// );
    pub fn decode_payload(input: &str) -> Result<Vec<Packet>, ParsePacketError> {
        if let Ok((_, packets)) = parse_string_payload(input) {
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

    /// Encode a packet with string data to a UTF-8 string.
    ///
    /// # Arguments
    /// * `input` - A `Packet` with string data
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///
    /// assert_eq!(
    ///     encode_packet(&Packet {
    ///         packet_type: PacketType::Message,
    ///         data: "Hello world!".into(),
    ///     }),
    ///     "4Hello world!",
    /// );
    ///
    /// ```
    pub fn encode_packet(input: &Packet) -> String {
        serialize_packet(input)
    }

    /// Encode a packet with binary data to a UTF-8 string.
    /// Note: calling `encode_payload` with Packets containing binary data
    /// also results in base64-encoded output.
    ///
    /// # Arguments
    /// * `input` - A `Packet` with binary data
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///
    /// assert_eq!(
    ///     encode_base64_packet(&Packet {
    ///        packet_type: PacketType::Message,
    ///        data: vec![1u8, 2u8, 3u8, 4u8].into(),
    ///     }),
    ///     "4AQIDBA=="
    /// );
    /// ```
    pub fn encode_base64_packet(input: &Packet) -> String {
        serialize_packet(&input)
    }

    /// Encode a payload containing multiple packets with either binary or string
    /// data to a UTF-8 string.
    ///
    /// # Arguments
    /// * `input`  - A list of `Packet`s with either string or binary data each
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketData, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///
    ///  assert_eq!(
    ///     encode_payload(&[
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: "€".into(),
    ///         },
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: vec![1u8, 2u8, 3u8, 4u8].into(),
    ///         }
    ///     ]),
    ///     "2:4€10:b4AQIDBA=="
    /// );
    /// ```
    pub fn encode_payload(input: &[Packet]) -> String {
        serialize_payload(input)
    }
}

fn parse_string_packet<'a>(input: &'a str, data_length: Option<usize>) -> IResult<&'a str, Packet> {
    let (input, packet_type_index) = take_u8(input)?;
    let (input, data) = match data_length {
        Some(l) => take(l)(input)?,
        None => ("", input),
    };
    let packet_type = PacketType::parse_from_u8(packet_type_index, input)?;
    Ok((
        input,
        Packet {
            packet_type,
            data: PacketData::Plaintext(Cow::Borrowed(data)),
        },
    ))
}

fn parse_string_payload(input: &str) -> IResult<&str, Vec<Packet>> {
    fold_many0(
        parse_string_packet_in_payload,
        Vec::new(),
        |mut acc: Vec<Packet>, item| {
            acc.push(item);
            acc
        },
    )(input)
}

fn parse_string_packet_in_payload(input: &str) -> IResult<&str, Packet> {
    // string packet length in a payload is the actual packet data character length + 1 (packet type)
    let (input, packet_length) = take_usize(input)?;
    // Take the colon which is the string separator
    let (input, _) = nom_char(':')(input)?;
    // packet length also contains the packet type, but we're sending the length of the data part.
    let data_length = (packet_length - 1) as usize;
    let (input, packet) = alt((
        parse_plaintext_string_packet(data_length),
        parse_base64_string_packet(data_length),
    ))(input)?;
    Ok((input, packet))
}

fn parse_plaintext_string_packet(data_length: usize) -> impl Fn(&str) -> IResult<&str, Packet> {
    move |input: &str| {
        let (input, packet) = parse_string_packet(input, Some(data_length))?;
        Ok((input, packet))
    }
}

fn parse_base64_string_packet(data_length: usize) -> impl Fn(&str) -> IResult<&str, Packet> {
    move |input: &str| {
        // base64-encoded data starts with b and counts for the data length
        let (input, _) = nom_char('b')(input)?;
        let data_length = data_length - 1;
        let (input, packet_type_index) = take_u8(input)?;
        let (input, data_b64) = take(data_length)(input)?;
        let packet_type = PacketType::parse_from_u8(packet_type_index, input)?;
        match base64::decode(data_b64) {
            Ok(vec) => Ok((
                input,
                Packet {
                    packet_type,
                    data: PacketData::Binary(Cow::Owned(vec)),
                },
            )),
            Err(_) => Err(Failure((input, nom::error::ErrorKind::TakeWhile1))),
        }
    }
}

fn serialize_packet(packet: &Packet) -> String {
    let packet_type_int = packet.packet_type as u8;
    let data: String = match &packet.data {
        PacketData::Plaintext(data) => data.to_string(),
        PacketData::Binary(data) => base64::encode(data),
        PacketData::Empty => String::from(""),
    };
    packet_type_int.to_string() + &data
}

fn serialize_payload(packets: &[Packet]) -> String {
    packets.iter().fold(String::from(""), |mut acc, packet| {
        let serialized = serialize_packet(&packet);
        acc.push_str(
            &(match &packet.data {
                PacketData::Binary(_) => {
                    // Adding + 1 to the length because of the `b` character indicating base64-encoded data.
                    (serialized.chars().count() + 1).to_string() + ":b" + &serialized
                }
                PacketData::Plaintext(_) => {
                    serialized.chars().count().to_string() + ":" + &serialized
                }
                PacketData::Empty => String::from(""),
            }),
        );
        acc
    })
}

fn take_u8(input: &str) -> IResult<&str, u8> {
    let (input, num) = digit1(input)?;
    if let Ok(num) = num.parse::<u8>() {
        Ok((input, num))
    } else {
        Err(Failure((input, nom::error::ErrorKind::Digit)))
    }
}

fn take_usize(input: &str) -> IResult<&str, usize> {
    let (input, num) = digit1(input)?;
    if let Ok(num) = num.parse::<usize>() {
        Ok((input, num))
    } else {
        Err(Failure((input, nom::error::ErrorKind::Digit)))
    }
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
            Err(nom::Err::Error(("hello", nom::error::ErrorKind::Digit)))
        );
    }

    #[test]
    fn test_take_usize() {
        assert_eq!(take_usize("0"), Ok(("", 0)));
        assert_eq!(take_usize("424242"), Ok(("", 424242)));
        assert_eq!(take_usize("0h2"), Ok(("h2", 0)));
    }
}
