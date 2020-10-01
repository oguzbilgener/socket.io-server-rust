use socket_io_parser::packet::PacketDataValue;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Handshake {
    /// The headers send as part of the handshake
    headers: HashMap<String, String>,
    ///The date of creation (as string)
    time: String,
    /// The ip of the client
    address: String,
    /// Whether the connection is cross-domain
    xdomain: bool,
    /// Whether the connection is secure
    secure: bool,
    /// The date of creation (as unix timestamp)
    issued: u32,
    /// The request URL string
    url: String,
    /// The query object
    query: HashMap<String, Vec<String>>,
    // The auth object
    auth: PacketDataValue,
}

pub struct Socket {
    id: String,
    handshake: Handshake,
}
