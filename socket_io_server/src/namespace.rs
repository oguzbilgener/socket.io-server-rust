use crate::socket::Socket;
use crate::storage::Storage;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::broadcast;
use socket_io_parser::PacketDataValue;

pub enum NamespaceEvent {
    // TODO: is this necessary?
    Broadcast,
}

pub struct NamespaceFlags {
    pub broadcast: bool,
    pub volatile: bool,
    pub local: bool,
}

impl Default for NamespaceFlags {
    fn default() -> Self {
        Self {
            broadcast: false,
            volatile: false,
            local: false,
        }
    }
}

pub struct NamespaceState {
    sockets: HashMap<String, Socket>,
    rooms: HashSet<String>,
    ids: usize,
}

impl NamespaceState {
    pub fn new() -> Self {
        Self {
            sockets: HashMap::new(),
            rooms: HashSet::new(),
            ids: 0,
        }
    }
}

pub struct Namespace<S>
where
    S: 'static + Storage,
{
    name: String,
    state: NamespaceState,
    storage: Arc<S>,
    // event_sender: broadcast::Sender<NamespaceEvent>,
}

impl<S> Namespace<S>
where
    S: 'static + Storage,
{
    pub fn new(name: String) -> Self {
        Self {
            name,
            state: NamespaceState::new(),
            storage: Arc::new(S::new()),
        }
    }

    pub fn add_connection(&self, connection_id: String, auth: PacketDataValue) {
        todo!();
    }
}
