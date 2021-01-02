use crate::error::{BroadcastError, EmitError};
use crate::socket::{Handshake, Socket};
use crate::storage::{Flags as StorageFlags, Storage};
use dashmap::{DashMap, DashSet};
use regex::Regex;
use socket_io_parser::packet::{Packet, PacketDataValue, PacketType};
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

pub type DynamicNamespaceNameMatchFn = dyn Fn(&str, &PacketDataValue) -> bool + Send + Sync;
const MESSAGE_EVENT_NAME: &str = "message";

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

/// The internal parts of a Namespace, made public only to satisfy the type bounds
pub trait NamespacePriv: Sized {
    // /// Add socket to the namespace
    // fn add_socket(&self, socket_id: &str);
    // /// Add socket to the namespace if it isn't in already, and to the specified room
    // fn add_socket_to_room(&self, socket_id: &str, room: &str);
    // /// Remove socket from this namespace completely
    // fn remove_socket(&self, socket_id: &str);
    // /// Remove socket from a room given by the name
    // fn remove_socket_from_room(&self, socket_id: &str, room: &str);

    fn broadcast_packet(
        &self,
        event_name: &str,
        data: PacketDataValue,
        rooms: &[String],
        flags: StorageFlags,
    ) -> Result<(), BroadcastError>;
}

pub trait Namespace<S>: NamespacePriv
where
    S: 'static + Storage,
{
    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent>;
    /// Filter this namespace by targeting the clients that are connected to the given `room_name`.
    fn to(self: Arc<Self>, room_name: &str) -> FilteredNamespace<Self>;
    /// Filter this namespace by targeting the clients that are connected to the given `room_name`.
    fn filter(self, room_name: &str) -> FilteredNamespace<Self>;
    /// Send an event to all clients
    fn emit(&self, event_name: &str, data: PacketDataValue) -> Result<(), EmitError>;
    /// Send an event to all clients, with specific storage flags
    fn emit_with_flags(
        &self,
        event_name: &str,
        data: PacketDataValue,
        flags: StorageFlags,
    ) -> Result<(), EmitError>;
    /// Send a `message` event to all clients
    fn send(&self, data: PacketDataValue) -> Result<(), EmitError>;
    /// Send a `message` event to all clients, with specific storage flags
    fn send_with_flags(&self, data: PacketDataValue, flags: StorageFlags) -> Result<(), EmitError>;
}

pub struct SimpleNamespace<S>
where
    S: 'static + Storage,
{
    pub name: String,
    // Only storing the actual sockets in the server
    pub(crate) socket_ids: DashSet<String>,
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
            ids: AtomicUsize::new(0),
            storage: Arc::new(S::new()),
            event_sender,
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub(crate) fn send_event(
        &self,
        event: NamespaceEvent,
    ) -> Result<usize, broadcast::SendError<NamespaceEvent>> {
        self.event_sender.send(event)
    }

    pub(crate) fn add_socket_to_room(&self, socket_id: &str, room: &str) {
        self.storage
            .add_all(socket_id, make_hash_set_from_str(room));
        self.socket_ids.insert(socket_id.to_owned());
        let _ = self.send_event(NamespaceEvent::Connection {
            socket_id: socket_id.to_owned(),
        });
    }

    pub(crate) fn add_socket(&self, socket_id: &str) {
        self.add_socket_to_room(socket_id, socket_id)
    }

    pub(crate) fn remove_socket(&self, socket_id: &str) {
        self.socket_ids.remove(socket_id);
        self.storage.del(socket_id, socket_id);
    }

    pub(crate) fn remove_socket_from_room(&self, socket_id: &str, room: &str) {
        self.storage.del(socket_id, room);
    }
}

impl<S> Namespace<S> for SimpleNamespace<S>
where
    S: 'static + Storage,
{
    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent> {
        self.event_sender.subscribe()
    }

    fn to(self: Arc<Self>, room_name: &str) -> FilteredNamespace<Self> {
        FilteredNamespace::new(room_name, self.clone())
    }

    fn filter(self, room_name: &str) -> FilteredNamespace<Self> {
        FilteredNamespace::new(room_name, Arc::new(self))
    }

    fn emit(&self, event_name: &str, data: PacketDataValue) -> Result<(), EmitError> {
        self.emit_with_flags(event_name, data, StorageFlags::default())
    }

    fn emit_with_flags(
        &self,
        event_name: &str,
        data: PacketDataValue,
        flags: StorageFlags,
    ) -> Result<(), EmitError> {
        let rooms: [String; 0] = [];
        self.broadcast_packet(event_name, data, &rooms, flags)
            .map_err(|err| err.into())
    }

    fn send(&self, data: PacketDataValue) -> Result<(), EmitError> {
        self.send_with_flags(data, StorageFlags::default())
    }

    fn send_with_flags(&self, data: PacketDataValue, flags: StorageFlags) -> Result<(), EmitError> {
        self.emit_with_flags(MESSAGE_EVENT_NAME, data, flags)
    }
}

impl<S> NamespacePriv for SimpleNamespace<S>
where
    S: 'static + Storage,
{
    fn broadcast_packet(
        &self,
        event_name: &str,
        data: PacketDataValue,
        rooms: &[String],
        flags: StorageFlags,
    ) -> Result<(), BroadcastError> {
        let packet = make_packet_for_event(event_name, data, self.get_name());
        // TODO: handle the error
        self.storage.broadcast(packet, rooms, flags)
    }
}

/// This is related to the `ParentNamespace` from the original socket.io JS implementation
pub struct DynamicNamespace<S>
where
    S: 'static + Storage,
{
    members: DashMap<String, Arc<SimpleNamespace<S>>>,
    event_sender: broadcast::Sender<NamespaceEvent>,
}

impl<S> DynamicNamespace<S>
where
    S: 'static + Storage,
{
    pub fn new() -> Self {
        // TODO: make the channel size parameterized
        let (event_sender, _) = broadcast::channel(128);

        DynamicNamespace {
            members: DashMap::new(),
            event_sender,
        }
    }

    /// This is akin to all the event listener setups in the
    /// `parent-namespace.ts/ParentNamespace::createChild()` method of
    /// the original implementation
    pub(crate) fn insert_simple_namespace(&self, simple_namespace: Arc<SimpleNamespace<S>>) {
        self.members.insert(
            simple_namespace.get_name().to_owned(),
            simple_namespace.clone(),
        );
    }
}

impl<S> Namespace<S> for DynamicNamespace<S>
where
    S: 'static + Storage,
{
    fn subscribe(&self) -> broadcast::Receiver<NamespaceEvent> {
        self.event_sender.subscribe()
    }

    fn to(self: Arc<Self>, room_name: &str) -> FilteredNamespace<Self> {
        FilteredNamespace::new(room_name, self)
    }

    fn filter(self, room_name: &str) -> FilteredNamespace<Self> {
        FilteredNamespace::new(room_name, Arc::new(self))
    }

    fn emit(&self, event_name: &str, data: PacketDataValue) -> Result<(), EmitError> {
        self.emit_with_flags(event_name, data, StorageFlags::default())
    }

    fn emit_with_flags(
        &self,
        event_name: &str,
        data: PacketDataValue,
        flags: StorageFlags,
    ) -> Result<(), EmitError> {
        let rooms: [String; 0] = [];
        self.broadcast_packet(event_name, data, &rooms, flags)
            .map_err(|err| err.into())
    }

    fn send(&self, data: PacketDataValue) -> Result<(), EmitError> {
        self.emit(MESSAGE_EVENT_NAME, data)
    }

    fn send_with_flags(&self, data: PacketDataValue, flags: StorageFlags) -> Result<(), EmitError> {
        self.emit_with_flags(MESSAGE_EVENT_NAME, data, flags)
    }
}

impl<S> NamespacePriv for DynamicNamespace<S>
where
    S: 'static + Storage,
{
    fn broadcast_packet(
        &self,
        event_name: &str,
        data: PacketDataValue,
        rooms: &[String],
        flags: StorageFlags,
    ) -> Result<(), BroadcastError> {
        self.members.iter().try_for_each(|el| {
            let member = el.value();
            let packet = make_packet_for_event(event_name, data.clone(), member.get_name());
            member.storage.broadcast(packet, rooms, flags)
        })
    }
}

/// A namespace that targets one or more rooms
///
/// # Example
/// ```rust
///
/// ```
///
#[derive(Debug, Clone)]
pub struct FilteredNamespace<N>
where
    N: 'static + NamespacePriv + Sized,
{
    namespace: Arc<N>,
    rooms: HashSet<String>,
}

impl<N> FilteredNamespace<N>
where
    N: 'static + NamespacePriv,
{
    pub(crate) fn new(room: &str, namespace: Arc<N>) -> Self {
        let mut rooms = HashSet::with_capacity(1);
        rooms.insert(room.to_owned());
        Self { namespace, rooms }
    }

    /// Expand the target audience by including another room name
    pub fn to(&self, room_name: &str) -> FilteredNamespace<N> {
        Self {
            namespace: self.namespace.clone(),
            rooms: make_hash_set_from_str(room_name),
        }
    }

    /// Expand the target audience by including another room name
    /// This an the alternative to `FilteredNamespace::to` that consumes `self`.
    pub fn add_room(self, room_name: &str) -> FilteredNamespace<N> {
        let mut rooms = self.rooms;
        rooms.insert(room_name.to_owned());
        Self {
            namespace: self.namespace,
            rooms,
        }
    }

    pub fn emit(&self, event_name: &str, data: PacketDataValue) -> Result<(), EmitError> {
        self.emit_with_flags(event_name, data, StorageFlags::default())
    }

    pub fn emit_with_flags(
        &self,
        event_name: &str,
        data: PacketDataValue,
        flags: StorageFlags,
    ) -> Result<(), EmitError> {
        if event_name.is_empty() {
            return Err(EmitError::EmptyEventName);
        }
        let rooms: Vec<String> = self.rooms.iter().cloned().collect();
        self.namespace
            .broadcast_packet(event_name, data, &rooms, flags)
            .map_err(|err| err.into())
    }

    pub fn send(&self, data: PacketDataValue) -> Result<(), EmitError> {
        self.emit(MESSAGE_EVENT_NAME, data)
    }

    pub fn send_with_flags(
        &self,
        data: PacketDataValue,
        flags: StorageFlags,
    ) -> Result<(), EmitError> {
        self.emit_with_flags(MESSAGE_EVENT_NAME, data, flags)
    }
}

fn make_hash_set_from_str(input_str: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(input_str.to_owned());
    set
}

fn make_packet_for_event(event_name: &str, data: PacketDataValue, namespace_name: &str) -> Packet {
    let name = PacketDataValue::String(event_name.to_owned());
    let packet_data = PacketDataValue::Array(Vec::from([name, data]));
    Packet {
        packet_type: PacketType::Event,
        data: packet_data,
        nsp: namespace_name.to_owned(),
        id: None,
    }
}
