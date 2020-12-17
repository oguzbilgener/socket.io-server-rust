use chrono::{DateTime, Utc};
use engine_io_server::util::RequestContext;
use socket_io_parser::packet::PacketDataValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Connectivity {
    Connected,
    Disconnected,
    // TODO: do we even need this, or would dropping Socket mean exactly this?
    Closed,
}

#[derive(Debug, Clone)]
pub struct Handshake {
    /// The headers send as part of the handshake
    pub headers: HashMap<String, String>,
    ///The date of creation
    pub time: DateTime<Utc>,
    /// The ip of the client
    pub address: String,
    /// Whether the connection is cross-domain
    pub xdomain: bool,
    /// Whether the connection is secure
    pub secure: bool,
    /// The date of creation (as unix timestamp)
    pub issued: i64,
    /// The request URL string
    pub url: String,
    /// The query object, simplified
    pub query: HashMap<String, String>,
    // The auth object
    pub auth: PacketDataValue,
}

impl Handshake {
    pub fn new(context: &RequestContext, auth: PacketDataValue) -> Self {
        let time = chrono::Utc::now();
        Self {
            headers: context.headers.clone(),
            time,
            xdomain: context.origin != None,
            address: context.remote_address.clone(),
            secure: context.secure,
            issued: time.timestamp(),
            url: context.request_url.clone(),
            query: context.query.clone(),
            auth,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SocketState {
    /// This is a combination of the ambiguous `connected `and `disconnected` flags
    /// from the original socket.io JS implementation
    connectivity: Connectivity,
}

impl SocketState {
    pub fn new() -> Self {
        SocketState {
            connectivity: Connectivity::Connected,
        }
    }
}

pub struct Socket {
    pub id: String,
    // TODO: do we really need this?
    state: Arc<RwLock<SocketState>>,
    handshake: Handshake,
}

impl Socket {
    pub fn new(handshake: Handshake) -> Self {
        // don't reuse the Engine.IO id because it's sensitive information
        let id = Self::generate_id();
        // TODO: handle the on_connect logic here:  join() and packet() to send the connect packet
        Socket {
            id,
            state: Arc::new(RwLock::new(SocketState::new())),
            handshake,
        }
    }

    // TODO: consider making a type/struct like SocketId that converts into/from a string
    // Just so we distinguish from the Engine.io's connection_id everywhere
    pub fn generate_id() -> String {
        Uuid::new_v4().to_hyphenated().to_string()
    }

    pub fn get_handshake(&self) -> &Handshake {
        &self.handshake
    }

    pub fn send_packet() {

    }

    // TODO: implement `_onclose` somewhere?

}
