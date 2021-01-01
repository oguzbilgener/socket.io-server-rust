use chrono::{DateTime, Utc};
use crate::namespace::SimpleNamespace;
use engine_io_server::util::RequestContext;
use socket_io_parser::packet::PacketDataValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

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
    pub(crate) fn new(context: &RequestContext, auth: PacketDataValue) -> Self {
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
}

impl SocketState {
    pub fn new() -> Self {
        SocketState {

        }
    }
}

pub struct Socket {
    pub id: String,
    // TODO: do we really need this?
    state: Arc<Mutex<SocketState>>,
    namespace: String,
    handshake: Handshake,
}

impl Socket {
    pub(crate) fn new(handshake: Handshake, namespace: &str) -> Self {
        // don't reuse the Engine.IO id because it's sensitive information
        let id = Self::generate_id();
        // TODO: handle the on_connect logic here:  join() and packet() to send the connect packet
        Socket {
            id,
            state: Arc::new(Mutex::new(SocketState::new())),
            handshake,
            namespace: namespace.to_owned(),
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
