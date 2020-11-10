use crate::socket::{Handshake, Socket};
use crate::storage::Storage;
use dashmap::{DashMap, DashSet};
use engine_io_server::adapter::Adapter;
use socket_io_parser::PacketDataValue;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::broadcast;

pub enum NamespaceEvent {
    Broadcast,
    Connection { socket_id: String },
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

pub struct Namespace<S>
where
    S: 'static + Storage,
{
    pub name: String,
    sockets: DashMap<String, Socket>,
    rooms: DashSet<String>,
    ids: AtomicUsize,
    storage: Arc<S>,
    event_sender: broadcast::Sender<NamespaceEvent>,
}

impl<S> Namespace<S>
where
    S: 'static + Storage,
{
    pub fn new(name: String) -> Self {
        // TODO: make the channel size parameterized
        let (event_sender, _) = broadcast::channel(128);

        Self {
            name,
            sockets: DashMap::new(),
            rooms: DashSet::new(),
            ids: AtomicUsize::new(0),
            storage: Arc::new(S::new()),
            event_sender,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent> {
        self.event_sender.subscribe()
    }

    /// Returns the new socket id
    pub fn add_connection(&self, handshake: Handshake) -> String {
        let socket = Socket::new(handshake);
        let socket_id = socket.id.clone();

        self.join_socket(&socket_id, &socket_id);

        // Assuming the client is still connected. If there is a disconnect event,
        // it should be coming from the same event queue (ClientEvent)
        self.sockets.insert(socket.id.clone(), socket);

        self.event_sender.send(NamespaceEvent::Connection {
            socket_id: socket_id.clone(),
        });
        socket_id
    }

    pub fn remove_connection(&self, socket_id: &str) {
        self.sockets.remove(socket_id);
    }

    pub fn to(self: Self, room_name: String) -> Self {
        self.rooms.insert(room_name);
        self
    }

    fn join_socket(&self, socket_id: &str, room: &str) {
        self.storage.add_all(socket_id, new_hash_set_from_str(room));
    }

    fn leave_socket(&self, socket_id: &str, room: &str) {
        self.storage.del(socket_id, room);
    }
}

fn new_hash_set_from_str(input_str: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(input_str.to_owned());
    set
}
