use crate::connection::{AddToNamespaceSuccess, Connection, DecodeBinarySuccess};
use crate::namespace::{
    DynamicNamespace, DynamicNamespaceNameMatchFn, Namespace, NamespaceDescriptor, NamespaceEvent,
    NamespaceKind, SimpleNamespace,
};
use crate::socket::Handshake;
use crate::storage::Storage;
use core::default::Default;
use dashmap::DashMap;
use engine_io_server::adapter::{Adapter, ListenOptions};
use engine_io_server::packet::Packet as EnginePacket;
use engine_io_server::server::BUFFER_CONST;
use engine_io_server::server::{ServerEvent as EngineEvent, ServerOptions as EngineOptions};
use engine_io_server::util::RequestContext;
use serde_json::json;
use socket_io_parser::decoder::Decoder;
use socket_io_parser::encoder::Encoder;
use socket_io_parser::engine_io_parser::EnginePacketData;
use socket_io_parser::packet::{Packet, PacketDataValue, PacketType, ProtoPacket};
use socket_io_parser::Parser;
use std::fmt::Display;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::broadcast;

pub struct ServerState<S>
where
    S: 'static + Storage,
{
    dynamic_namespaces: Vec<(Box<DynamicNamespaceNameMatchFn>, Arc<DynamicNamespace<S>>)>,
    event_receiver_temp: Option<broadcast::Receiver<ServerEvent>>,
}

impl<S> ServerState<S>
where
    S: 'static + Storage,
{
    fn new(event_receiver: broadcast::Receiver<ServerEvent>) -> Self {
        ServerState {
            dynamic_namespaces: Vec::new(),
            event_receiver_temp: Some(event_receiver),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerOptions {
    /// in milliseconds
    pub connect_timeout: u32,
    pub buffer_factor: usize,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            connect_timeout: 45000,
            buffer_factor: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    Error { message: String },
}

pub struct Options<A>
where
    A: 'static + Adapter,
{
    pub engine_options: EngineOptions,
    pub adapter_options: A::Options,
    pub listen_options: ListenOptions,
    pub socketio_options: ServerOptions,
}

impl<A> Default for Options<A>
where
    A: 'static + Adapter,
{
    fn default() -> Self {
        Options {
            engine_options: EngineOptions::default(),
            adapter_options: A::Options::default(),
            listen_options: ListenOptions {
                path: "/socket.io",
                ..ListenOptions::default()
            },
            socketio_options: ServerOptions::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    Connect {
        // Engine Socket ID
        connection_id: String,
        context: Arc<RequestContext>,
        nsp: String,
        data: PacketDataValue,
    },
    Packet {
        connection_id: String,
        context: Arc<RequestContext>,
        packet: Packet,
    },
    Error,
}

pub struct Shared<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    adapter: A,
    state: RwLock<ServerState<S>>,
    /// The main place that all the connected clients are stored. engine_connection_id => Connection
    connections: DashMap<String, Connection<S, P::Decoder>>,
    /// A mapping of name => Namespace
    namespaces: DashMap<String, Arc<SimpleNamespace<S>>>,
    /// A mapping of socket_id => engine_connection_id
    connection_lookup: DashMap<String, String>,
    event_sender: broadcast::Sender<ServerEvent>,
}

pub struct Server<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    shared: Arc<Shared<A, S, P>>,
    socketio_options: ServerOptions,
}

impl<A, S, P> Server<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    pub fn new(options: Options<A>) -> Self {
        let adapter = A::new(options.engine_options, options.adapter_options);
        let (event_sender, event_receiver) = broadcast::channel::<ServerEvent>(
            options.socketio_options.buffer_factor * BUFFER_CONST,
        );
        let shared = Shared {
            adapter,
            state: RwLock::new(ServerState::new(event_receiver)),
            connections: DashMap::new(),
            namespaces: DashMap::new(),
            connection_lookup: DashMap::new(),
            event_sender,
        };
        Server {
            shared: Arc::new(shared),
            socketio_options: options.socketio_options,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        let event_receiver_temp = self
            .shared
            .state
            .write()
            .unwrap()
            .event_receiver_temp
            .take();
        if let Some(event_receiver) = event_receiver_temp {
            self.shared.clone().subscribe_to_engine_events();
            event_receiver
        } else {
            self.shared.event_sender.subscribe()
        }
    }

    /// Sends a message to a single socket client, if connected
    pub fn send_message(
        &self,
        socket_id: &str,
        data: PacketDataValue,
    ) -> Result<(), NotFoundError> {
        todo!();
    }

    pub fn send_packet_to_namespace(
        &self,
        namespace_name: &str,
        data: PacketDataValue,
    ) -> Result<(), NotFoundError> {
        todo!();
    }

    pub fn get_namespace(&self, name: &str) -> Arc<SimpleNamespace<S>> {
        self.shared.clone().get_or_create_namespace(name).0
    }

    pub fn make_dynamic_namespace(
        &self,
        namespace_descriptor: NamespaceDescriptor,
    ) -> Arc<DynamicNamespace<S>> {
        let match_fn = match namespace_descriptor {
            NamespaceDescriptor::Function(f) => f,
            NamespaceDescriptor::Regex(regex) => {
                Box::new(move |name: &str, _auth: &PacketDataValue| -> bool {
                    regex.is_match(name)
                })
            }
            NamespaceDescriptor::Text(text) => {
                Box::new(move |name: &str, _auth: &PacketDataValue| -> bool { name == text })
            }
        };
        let dynamic_namespace = Arc::new(DynamicNamespace::new());
        self.shared
            .state
            .write()
            .unwrap()
            .dynamic_namespaces
            .push((match_fn, dynamic_namespace.clone()));
        dynamic_namespace
    }

    pub fn of(&self, namespace_descriptor: NamespaceDescriptor) -> NamespaceKind<S> {
        match namespace_descriptor {
            NamespaceDescriptor::Text(name) => NamespaceKind::Simple(self.get_namespace(&name)),
            _ => NamespaceKind::Dynamic(self.make_dynamic_namespace(namespace_descriptor)),
        }
    }

    pub fn join_room(&self, socket_id: &str, room_name: &str) {
        // TODO: how to implement this?
        todo!()
    }
}

impl<A, S, P> Shared<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    fn get_or_create_namespace(
        self: Arc<Self>,
        name: &str,
    ) -> (Arc<SimpleNamespace<S>>, GetResult) {
        let name = if !name.starts_with('/') {
            "/".to_owned() + name
        } else {
            name.to_owned()
        };
        let namespace = self.namespaces.get(&name);
        if let Some(namespace) = namespace {
            (namespace.clone(), GetResult::Existing)
        } else {
            let namespace = Arc::new(SimpleNamespace::<S>::new(name.clone()));
            self.namespaces.insert(name, namespace.clone());
            (namespace, GetResult::New)
        }
    }

    fn subscribe_to_engine_events(self: Arc<Self>) {
        let mut engine_event_listener = self.clone().adapter.subscribe();

        tokio::spawn(async move {
            let shared = self.clone();
            while let Ok((message, responder)) = engine_event_listener.recv().await {
                let shared = shared.clone();
                tokio::spawn(async move {
                    match message {
                        // socket.io/lib/index.ts :: onconnection
                        EngineEvent::Connection { connection_id } => {
                            dbg!("new engine connection from {}!", &connection_id);
                            shared.clone().setup_connect_timer(connection_id).await;
                        }
                        EngineEvent::Close {
                            connection_id,
                            reason,
                        } => {
                            dbg!(
                                "engine socket disconnected id: {}, reason: {}",
                                &connection_id,
                                &reason
                            );
                            shared.clone().handle_engine_socket_close(&connection_id);
                        }
                        // socket.io/lib/client.ts :: ondata
                        EngineEvent::Message {
                            connection_id,
                            context,
                            data,
                        } => {
                            shared
                                .clone()
                                .handle_new_message(context, connection_id, data, responder)
                                .await
                        }
                        // socket.io/lib/client.ts :: onerror
                        EngineEvent::Error { connection_id } => {
                            // TODO: provide more details about the error
                            shared.clone().adapter.close_socket(&connection_id).await;
                        }
                        _ => {
                            // TODO: what to do with other events?
                        }
                    }
                });
            }
        });
    }

    fn handle_engine_socket_close(self: Arc<Self>, engine_connection_id: &str) {
        if self.remove_connection(engine_connection_id) {
            dbg!(
                "Handled socket close for connection_id = {:?}",
                engine_connection_id
            );
        } else {
            dbg!("Attempted to remove socket record for connection_id = {:?} but no record was found", engine_connection_id);
        }
    }

    pub async fn handle_new_message(
        self: Arc<Self>,
        context: Arc<RequestContext>,
        engine_connection_id: String,
        data: EnginePacketData,
        responder: bmrng::Responder<Vec<EnginePacket>>,
    ) {
        println!("new message from {} {:?}", engine_connection_id, data);
        match data {
            EnginePacketData::Text(text) => {
                let proto_packet = match P::Decoder::decode_proto_packet(&text) {
                    Ok(p) => p,
                    Err(_err) => {
                        // TODO: should we send this error? check original implementation
                        let _ = self.event_sender.send(ServerEvent::Error {
                            message: "Bad packet".to_owned(),
                        });
                        return;
                    }
                };

                match proto_packet {
                    ProtoPacket::Multipart(header) => {
                        if header.has_attachments() {
                            if let Some(connection) =
                                self.clone().connections.get(&engine_connection_id)
                            {
                                if let Err(_err) = connection.set_decoder(P::Decoder::new(header)) {
                                    // TODO: is this good enough?
                                    let _ = self.event_sender.send(ServerEvent::Error {
                                        message: "sequence error".to_owned(),
                                    });
                                }
                            } else {
                                // TODO: propagate error? respond with error?
                                println!("connection not found");
                            }
                        } else {
                            // TODO: Ignore this? or log this"
                            //  binary_event or binary_ack packet with no attachment.
                            println!("binary packet has no attachments");
                        }
                    }
                    ProtoPacket::Plain(packet) => {
                        // "ondecoded" from client.ts
                        self.handle_packet_from_client(
                            context,
                            engine_connection_id,
                            packet,
                            responder,
                        )
                        .await;
                    }
                }
            }
            EnginePacketData::Binary(buffer) => {
                if let Some(connection) = self.clone().connections.get(&engine_connection_id) {
                    match connection.decode_binary_data(buffer) {
                        Ok(DecodeBinarySuccess::Done { packet }) => {
                            // another "ondecoded" from client.ts
                            self.handle_packet_from_client(
                                context,
                                engine_connection_id,
                                packet,
                                responder,
                            )
                            .await;
                        }
                        _ => {}
                    }
                } else {
                    dbg!("connection with id {:?} not found", engine_connection_id);
                }
            }
            EnginePacketData::Empty => {
                // TODO: this is considered unknown. Ignore? or reset the buffer?
            }
        }
    }

    /// Returns a packet to be sent back to the adapter/engine.io layer
    async fn handle_client_connect(
        self: Arc<Self>,
        context: Arc<RequestContext>,
        namespace_name: String,
        engine_connection_id: String,
        auth: PacketDataValue,
    ) -> Packet {
        let connect_result = if self.clone().namespace_exists(&namespace_name) {
            Ok(self.connect_client(
                context.clone(),
                &namespace_name,
                &engine_connection_id,
                auth,
            ))
        } else {
            if let Some(_dynamic_namespace) = self
                .clone()
                .get_matching_dynamic_namespace(&namespace_name, &auth)
                .await
            {
                Ok(self.connect_client(
                    context.clone(),
                    &namespace_name,
                    &engine_connection_id,
                    auth,
                ))
            } else {
                // TODO: proper error types!
                Err("Invalid namespace")
            }
        };

        match connect_result {
            Ok((namespace, socket_id)) => Packet {
                packet_type: PacketType::Connect,
                id: None,
                nsp: namespace.get_name().to_owned(),
                data: serde_json::json!({
                    "sid": socket_id,
                })
                .into(),
            },
            Err(err_message) => Packet {
                packet_type: PacketType::ConnectError,
                nsp: namespace_name,
                data: json!({ "message": err_message }).into(),
                id: None,
            },
        }
    }

    // `ondecoded` from client.ts
    async fn handle_packet_from_client(
        self: Arc<Self>,
        context: Arc<RequestContext>,
        engine_connection_id: String,
        packet: Packet,
        mut responder: bmrng::Responder<Vec<EnginePacket>>,
    ) {
        match packet.packet_type {
            PacketType::Connect => {
                // CONNECT or CONNECT_ERROR packet
                let connect_packet = self
                    .handle_client_connect(context, packet.nsp, engine_connection_id, packet.data)
                    .await;
                let encoded_packet = P::Encoder::encode_packet(connect_packet);
                let _ = responder.respond(encoded_packet.into());
            }
            _ => {
                dbg!("new socket.io packet from the client! {:?}", &packet);
                if let Some(connection) = self.connections.get(&engine_connection_id) {
                    // TODO: error handling?
                    connection.handle_packet(packet);
                } else {
                    dbg!("No connection found for engine_connection_id {:?}", &engine_connection_id);
                }
            }
        }
    }

    /// socket.io/lib/client.ts :: doConnect
    fn connect_client(
        self: Arc<Self>,
        context: Arc<RequestContext>,
        namespace_name: &String,
        engine_connection_id: &String,
        auth: PacketDataValue,
    ) -> (Arc<SimpleNamespace<S>>, String) {
        self.clone().clear_connect_timer(engine_connection_id);
        let namespace = self.clone().get_or_create_namespace(namespace_name).0;
        let handshake = Handshake::new(&context, auth);
        let socket_id = self.add_connection(namespace.clone(), engine_connection_id, handshake);
        (namespace, socket_id)
    }

    fn namespace_exists(self: Arc<Self>, name: &str) -> bool {
        self.namespaces.contains_key(name)
    }

    /// `server._checkNamespace()` in the original implementation
    async fn get_matching_dynamic_namespace(
        self: Arc<Self>,
        namespace_name: &str,
        auth: &PacketDataValue,
    ) -> Option<Arc<DynamicNamespace<S>>> {
        let state_ref = self.clone();
        let dynamic_namespaces = &state_ref.state.read().unwrap().dynamic_namespaces;
        if dynamic_namespaces.is_empty() {
            return None;
        }
        let dynamic_namespace = dynamic_namespaces
            .iter()
            .find(move |(check_fn, _)| check_fn(namespace_name, auth))
            .map(|dynamic_namespace| dynamic_namespace.1.clone());

        if let Some(ref dynamic_namespace) = dynamic_namespace {
            let (simple_namespace, get_result) = self.get_or_create_namespace(&namespace_name);
            if get_result == GetResult::New {
                // This part is akin to `parent-namespace.ts/createChild()`
                dynamic_namespace.insert_simple_namespace(simple_namespace);
            }
        }
        dynamic_namespace
    }

    /// Connects a client to a namespace
    /// Returns the new socket id
    fn add_connection(
        self: Arc<Self>,
        namespace: Arc<SimpleNamespace<S>>,
        engine_connection_id: &str,
        handshake: Handshake,
    ) -> String {
        let socket_id = if let Some(existing_connection) =
            self.connections.get(engine_connection_id)
        {
            match existing_connection.add_to_namespace(handshake, namespace.clone()) {
                AddToNamespaceSuccess::Added { socket_id } => {
                    dbg!(
                        "Created a new socket, joined namespace, socket_id = {:?}, name = {:?}",
                        &socket_id,
                        namespace.get_name()
                    );
                    socket_id
                }
                AddToNamespaceSuccess::AlreadyExisting { socket_id } => {
                    dbg!("Attempted to add a connection that's already connected to a namespace. connection_id = {:?}, namespace = {:?}", engine_connection_id, namespace.get_name());
                    socket_id
                }
            }
        } else {
            let (connection, socket_id) = Connection::<S, P::Decoder>::initialize(
                engine_connection_id,
                handshake,
                namespace.clone(),
            );
            self.connections.insert(engine_connection_id.to_owned(), connection);
            socket_id
        };
        self.connection_lookup
            .insert(socket_id.clone(), engine_connection_id.to_owned());
        socket_id
    }

    fn remove_connection(self: Arc<Self>, engine_connection_id: &str) -> bool {
        if let Some((_, mut connection)) = self.connections.remove(engine_connection_id) {
            connection.iter_socket_ids_mut().for_each(|socket_id| {
                self.connection_lookup.remove(socket_id);
            });
            // The rest of the cleanup (for namespaces is done in the destructor for the connection state)
            true
        } else {
            false
        }
    }

    async fn setup_connect_timer(self: Arc<Self>, connection_id: String) {
        // TODO: complete this
        todo!();
    }

    fn clear_connect_timer(self: Arc<Self>, connection_id: &str) {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum GetResult {
    New,
    Existing,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub struct NotFoundError;

impl Display for NotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", "Client or namespace not found")
    }
}

impl std::error::Error for NotFoundError {}
