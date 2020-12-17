use crate::socket::{Handshake, Socket};
use crate::storage::Storage;
use dashmap::{DashMap, DashSet};
use engine_io_server::adapter::Adapter;
use regex::Regex;
use socket_io_parser::packet::Packet;
use socket_io_parser::PacketDataValue;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::broadcast;

pub type DynamicNamespaceNameMatchFn = dyn Fn(&str, &PacketDataValue) -> bool + Send + Sync;

pub enum NamespaceDescriptor {
    Text(String),
    Regex(Regex),
    Function(Box<DynamicNamespaceNameMatchFn>),
}

pub enum NamespaceKind<S>
where
    S: 'static + Storage,
{
    Simple(Arc<SimpleNamespace<S>>),
    Dynamic(Arc<DynamicNamespace<S>>),
}

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

pub trait Namespace {
    fn get_name(&self) -> &str;
    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent>;
    fn to(self: Self, room_name: String) -> Self;
    fn join_socket(&self, socket_id: &str, room: &str);
    fn leave_socket(&self, socket_id: &str, room: &str);
}

pub struct SimpleNamespace<S>
where
    S: 'static + Storage,
{
    pub name: String,
    // Only storing the actual sockets in the server
    pub(crate) socket_ids: DashSet<String>,
    rooms: DashSet<String>,
    ids: AtomicUsize,
    storage: Arc<S>,
    event_sender: broadcast::Sender<NamespaceEvent>,
}

impl<S> SimpleNamespace<S>
where
    S: 'static + Storage,
{
    pub fn new(name: String) -> Self {
        // TODO: make the channel size parameterized
        let (event_sender, _) = broadcast::channel(128);

        Self {
            name,
            socket_ids: DashSet::new(),
            rooms: DashSet::new(),
            ids: AtomicUsize::new(0),
            storage: Arc::new(S::new()),
            event_sender,
        }
    }

    pub(crate) fn on_packet_from_client(&self, packet: Packet) {
        // TODO: distribute this to event to all the freaking Socket instances
        todo!()
    }

    pub(crate) fn send_event(&self, event: NamespaceEvent) -> Result<usize, broadcast::SendError<NamespaceEvent>> {
        self.event_sender.send(event)
    }
}

impl<S> Namespace for SimpleNamespace<S>
where
    S: 'static + Storage,
{
    fn get_name(&self) -> &str {
        &self.name
    }

    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent> {
        self.event_sender.subscribe()
    }

    fn to(self: Self, room_name: String) -> Self {
        self.rooms.insert(room_name);
        self
    }

    fn join_socket(&self, socket_id: &str, room: &str) {
        self.storage.add_all(socket_id, new_hash_set_from_str(room));
        self.socket_ids.insert(socket_id.to_owned());
    }

    fn leave_socket(&self, socket_id: &str, room: &str) {
        self.socket_ids.remove(socket_id);
        self.storage.del(socket_id, room);
    }
}

fn new_hash_set_from_str(input_str: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(input_str.to_owned());
    set
}

/// This is related to the `ParentNamespace` from the original socket.io JS implementation
pub struct DynamicNamespace<S>
where
    S: 'static + Storage,
{
    subscribed_namespaces: DashMap<String, Arc<SimpleNamespace<S>>>,
}

impl<S> DynamicNamespace<S>
where
    S: 'static + Storage,
{
    pub fn new() -> Self {
        DynamicNamespace {
            subscribed_namespaces: DashMap::new(),
        }
    }

    /// This is akin to all the event listener setups in the
    /// `parent-namespace.ts/ParentNamespace::createChild()` method of
    /// the original implementation
    pub(crate) fn connect_simple_namespace(&self, simple_namespace: Arc<SimpleNamespace<S>>) {
        // TODO: implement this Dec 29
        todo!()
    }
}

impl<S> Namespace for DynamicNamespace<S>
where
    S: 'static + Storage,
{
    fn get_name(&self) -> &str {
        todo!()
    }

    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent> {
        todo!()
    }

    fn to(self: Self, room_name: String) -> Self {
        todo!()
    }

    fn join_socket(&self, socket_id: &str, room: &str) {
        todo!()
    }

    fn leave_socket(&self, socket_id: &str, room: &str) {
        todo!()
    }
}
