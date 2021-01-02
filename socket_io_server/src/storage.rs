use socket_io_parser::packet::Packet;
use std::collections::HashSet;
use crate::error::BroadcastError;

pub trait Storage: 'static + Send + Sync + Sized {
    fn new() -> Self;

    fn add_all(&self, socket_id: &str, room_names: HashSet<String>);

    fn del(&self, socket_id: &str, room_name: &str);

    fn broadcast(
        &self,
        packet: Packet,
        rooms: &[String],
        flags: Flags,
    ) -> Result<(), BroadcastError>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flags {
    pub volatile: bool,
    pub compress: bool,
    pub local: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            volatile: false,
            compress: false,
            local: false,
        }
    }
}

pub struct DefaultStorage {}

impl Storage for DefaultStorage {
    fn new() -> Self {
        todo!()
    }

    fn add_all(&self, socket_id: &str, room_names: HashSet<String>) {
        todo!()
    }

    fn del(&self, socket_id: &str, room_name: &str) {
        todo!()
    }

    fn broadcast(
        &self,
        packet: Packet,
        rooms: &[String],
        flags: Flags,
    ) -> Result<(), BroadcastError> {
        todo!()
    }
}
