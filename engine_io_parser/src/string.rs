use crate::packet::{Packet, PacketType, ParsePacketError};
use nom::{
    branch::alt, bytes::complete::take, character::complete::char as nom_char,
    character::complete::digit1, multi::fold_many0, Err::Failure, IResult,
};

pub mod decoder {
    use super::*;

    /// In string-encoded payloads, each packet may contain string or binary data.
    /// Binary data in string-encoded payloads is encoded to base64, so
    /// Otherwise, string data is returned as is.
    /// Use this enum to figure out if a packet in a payload has binary data
    ///
    #[derive(Debug, Clone, PartialEq)]
    pub enum PacketData<'a> {
        PlaintextData(&'a str),
        BinaryData(Vec<u8>),
    }

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
    ///     data: "Hello world!",
    /// }))
    /// ```
    pub fn decode_packet(input: &str) -> Result<Packet<&str>, ParsePacketError> {
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
    ///
    /// use engine_io_parser::packet::{Packet, PacketType};
    /// use engine_io_parser::string::decoder::*;
    ///
    /// assert_eq!(
    ///     decode_payload("6:4hello2:4€"),
    ///     Ok(vec![
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: PacketData::PlaintextData("hello"),
    ///         },
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: PacketData::PlaintextData("€")
    ///         }
    ///     ]),
    /// );
    pub fn decode_payload(input: &str) -> Result<Vec<Packet<PacketData>>, ParsePacketError> {
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

    /// In string-encoded payloads, each packet may contain string or binary data.
    /// Binary data in string-encoded payloads is encoded to base64, so
    /// Otherwise, string data is returned as is.
    /// Use this enum to indicate if a packet in a payload has binary data
    ///
    #[derive(Debug, Clone, PartialEq)]
    pub enum PacketData<'a> {
        PlaintextData(&'a str),
        BinaryData(&'a [u8]),
    }

    /// Encode a packet with string data to a UTF-8 string.
    ///
    /// # Arguments
    /// * `input` - A `Packet` with string data
    ///
    /// # Example
    ///
    /// ```rust
    /// use engine_io_parser::packet::{Packet, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///
    /// assert_eq!(
    ///     encode_packet(Packet {
    ///         packet_type: PacketType::Message,
    ///         data: "Hello world!",
    ///     }),
    ///     "4Hello world!",
    /// );
    ///
    /// ```
    pub fn encode_packet(input: Packet<&str>) -> String {
        serialize_packet(input.packet_type, &PacketData::PlaintextData(input.data))
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
    /// use engine_io_parser::packet::{Packet, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///
    /// assert_eq!(
    ///     encode_base64_packet(Packet {
    ///        packet_type: PacketType::Message,
    ///        data: &[1u8, 2u8, 3u8, 4u8],
    ///     }),
    ///     "4AQIDBA=="
    /// );
    /// ```
    pub fn encode_base64_packet(input: Packet<&[u8]>) -> String {
        serialize_packet(input.packet_type, &PacketData::BinaryData(input.data))
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
    /// use engine_io_parser::packet::{Packet, PacketType};
    /// use engine_io_parser::string::encoder::*;
    ///  assert_eq!(
    ///     encode_payload(&[
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: PacketData::PlaintextData("€"),
    ///         },
    ///         Packet {
    ///             packet_type: PacketType::Message,
    ///             data: PacketData::BinaryData(&[1, 2, 3, 4])
    ///         }
    ///     ]),
    ///     "2:4€10:b4AQIDBA=="
    /// );
    /// ```
    pub fn encode_payload(input: &[Packet<PacketData>]) -> String {
        serialize_payload(input)
    }
}

fn parse_string_packet<'a>(
    input: &'a str,
    data_length: Option<usize>,
) -> IResult<&'a str, Packet<&'a str>> {
    let (input, packet_type_index) = take_u8(input)?;
    let (input, data) = match data_length {
        Some(l) => take(l)(input)?,
        None => ("", input),
    };
    let packet_type = PacketType::parse_from_u8(packet_type_index, input)?;
    Ok((input, Packet { packet_type, data }))
}

fn parse_string_payload(input: &str) -> IResult<&str, Vec<Packet<decoder::PacketData>>> {
    fold_many0(
        parse_string_packet_in_payload,
        Vec::new(),
        |mut acc: Vec<Packet<decoder::PacketData>>, item| {
            acc.push(item);
            acc
        },
    )(input)
}

fn parse_string_packet_in_payload(input: &str) -> IResult<&str, Packet<decoder::PacketData>> {
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

fn parse_plaintext_string_packet(
    data_length: usize,
) -> impl Fn(&str) -> IResult<&str, Packet<decoder::PacketData>> {
    move |input: &str| {
        let (input, packet) = parse_string_packet(input, Some(data_length))?;
        Ok((
            input,
            Packet {
                packet_type: packet.packet_type,
                data: decoder::PacketData::PlaintextData(packet.data),
            },
        ))
    }
}

fn parse_base64_string_packet(
    data_length: usize,
) -> impl Fn(&str) -> IResult<&str, Packet<decoder::PacketData>> {
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
                    data: decoder::PacketData::BinaryData(vec),
                },
            )),
            Err(_) => Err(Failure((input, nom::error::ErrorKind::TakeWhile1))),
        }
    }
}

fn serialize_packet(packet_type: PacketType, data: &encoder::PacketData) -> String {
    let packet_type_int = packet_type as u8;
    let data: String = match data {
        encoder::PacketData::PlaintextData(data) => data.to_string(),
        encoder::PacketData::BinaryData(data) => base64::encode(data),
    };
    packet_type_int.to_string() + &data
}

fn serialize_payload(packets: &[Packet<encoder::PacketData>]) -> String {
    packets.iter().fold(String::from(""), |acc, packet| {
        let serialized = serialize_packet(packet.packet_type, &packet.data);
        acc + &(match &packet.data {
            encoder::PacketData::BinaryData(_) => {
                // Adding + 1 to the length because of the `b` character indicating base64-encoded data.
                (serialized.chars().count() + 1).to_string() + ":b" + &serialized
            }
            encoder::PacketData::PlaintextData(_) => {
                serialized.chars().count().to_string() + ":" + &serialized
            }
        })
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
