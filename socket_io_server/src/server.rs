use crate::namespace::{
    DynamicNamespace, DynamicNamespaceNameMatchFn, Namespace, NamespaceDescriptor, NamespaceEvent,
    NamespaceKind, SimpleNamespace,
};
use crate::socket::{Handshake, Socket};
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
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::broadcast;

pub struct ServerState<D, S>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    namespaces: HashMap<String, Arc<SimpleNamespace<S>>>,
    dynamic_namespaces: Vec<(Box<DynamicNamespaceNameMatchFn>, Arc<DynamicNamespace<S>>)>,
    decoders_in_progress: HashMap<String, D>,
    event_receiver_temp: Option<broadcast::Receiver<ServerEvent>>,
}

impl<D, S> ServerState<D, S>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    fn new(event_receiver: broadcast::Receiver<ServerEvent>) -> Self {
        ServerState {
            namespaces: HashMap::new(),
            dynamic_namespaces: Vec::new(),
            decoders_in_progress: HashMap::new(),
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

pub struct Server<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    adapter: Arc<A>,
    storage: Arc<S>,
    socketio_options: ServerOptions,
    state: Arc<RwLock<ServerState<P::Decoder, S>>>,
    /// The main place that all the connected clients are stored
    sockets: Arc<DashMap<String, Socket>>,
    event_sender: broadcast::Sender<ServerEvent>,
}

impl<A, S, P> Server<A, S, P>
where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    pub fn new(options: Options<A>, storage: S) -> Self {
        let adapter = A::new(options.engine_options, options.adapter_options);
        let (event_sender, event_receiver) = broadcast::channel::<ServerEvent>(
            options.socketio_options.buffer_factor * BUFFER_CONST,
        );
        Server {
            adapter: Arc::new(adapter),
            storage: Arc::new(storage),
            socketio_options: options.socketio_options,
            state: Arc::new(RwLock::new(ServerState::new(event_receiver))),
            sockets: Arc::new(DashMap::new()),
            event_sender,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        let event_receiver_temp = self.state.write().unwrap().event_receiver_temp.take();
        if let Some(event_receiver) = event_receiver_temp {
            self.subscribe_to_engine_events();
            event_receiver
        } else {
            self.event_sender.subscribe()
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

    fn subscribe_to_engine_events(&self) {
        let mut engine_event_listener = self.adapter.subscribe();
        let state = self.state.clone();
        let server_event_sender = self.event_sender.clone();
        let adapter = self.adapter.clone();
        let sockets = self.sockets.clone();

        tokio::spawn(async move {
            while let Ok((message, mut responder)) = engine_event_listener.recv().await {
                let adapter = adapter.clone();
                let state = state.clone();
                let server_event_sender = server_event_sender.clone();
                let sockets = sockets.clone();
                tokio::spawn(async move {
                    match message {
                        // socket.io/lib/index.ts :: onconnection
                        EngineEvent::Connection { connection_id } => {
                            dbg!("new engine connection from {}!", &connection_id);
                            setup_connect_timer(connection_id).await;
                            let _ = responder.respond(None);
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
                            handle_engine_socket_close::<A, S, P>(state, sockets, connection_id);
                            let _ = responder.respond(None);
                        }
                        // socket.io/lib/client.ts :: ondata
                        EngineEvent::Message {
                            connection_id,
                            context,
                            data,
                        } => {
                            handle_new_message::<A, S, P>(
                                adapter,
                                state,
                                context,
                                sockets,
                                connection_id,
                                data,
                                server_event_sender.clone(),
                                responder,
                            )
                            .await
                        }
                        // socket.io/lib/client.ts :: onerror
                        EngineEvent::Error { connection_id } => {
                            // TODO: provide more details about the error
                            let _ = responder.respond(None);
                            adapter.close_socket(&connection_id).await;
                        }
                        _ => {
                            // TODO: what to do with other events?
                            let _ = responder.respond(None);
                        }
                    }
                });
            }
        });
    }

    pub fn get_namespace(&self, name: &str) -> Arc<SimpleNamespace<S>> {
        get_or_create_namespace(self.state.clone(), name).0
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
        let state = self.state.clone();
        state
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
}

pub async fn handle_new_message<A, S, P>(
    adapter: Arc<A>,
    state: Arc<RwLock<ServerState<P::Decoder, S>>>,
    context: Arc<RequestContext>,
    sockets: Arc<DashMap<String, Socket>>,
    engine_connection_id: String,
    data: EnginePacketData,
    server_event_sender: broadcast::Sender<ServerEvent>,
    responder: bmrng::Responder<Option<Vec<EnginePacket>>>,
) where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    println!("new message from {} {:?}", engine_connection_id, data);
    match data {
        EnginePacketData::Text(text) => {
            let proto_packet = match P::Decoder::decode_proto_packet(&text) {
                Ok(p) => p,
                Err(err) => {
                    // TODO: should we send this error? check original implementation
                    let _ = server_event_sender.send(ServerEvent::Error {
                        message: "Bad packet".to_owned(),
                    });
                    return;
                }
            };

            match proto_packet {
                ProtoPacket::Multipart(header) => {
                    if header.has_attachments() {
                        let mut state = state.write().unwrap();
                        if state
                            .decoders_in_progress
                            .contains_key(&engine_connection_id)
                        {
                            // TODO: is this good enough?
                            handle_message_processing_error(responder);
                            let _ = server_event_sender.send(ServerEvent::Error {
                                message: "sequence error".to_owned(),
                            });
                        } else {
                            state
                                .decoders_in_progress
                                .insert(engine_connection_id, P::Decoder::new(header));
                        }
                    } else {
                        // TODO: Ignore this? or log this"
                        //  binary_event or binary_ack packet with no attachment.
                        println!("binary packet has no attachments");
                    }
                }
                ProtoPacket::Plain(packet) => {
                    // "ondecoded" from client.ts
                    handle_packet_from_client::<A, S, P>(
                        adapter.clone(),
                        state,
                        context,
                        sockets,
                        engine_connection_id,
                        packet,
                        server_event_sender,
                        responder,
                    )
                    .await;
                }
            }
        }
        EnginePacketData::Binary(buffer) => {
            let server_state = state.clone();
            let decoded = state
                .write()
                .unwrap()
                .decoders_in_progress
                .get_mut(&engine_connection_id)
                .ok_or(P::Decoder::make_err())
                .and_then(|d| d.decode_binary_data(buffer));
            match decoded {
                Ok(done) => {
                    if done {
                        let decoder = state
                            .write()
                            .unwrap()
                            .decoders_in_progress
                            .remove(&engine_connection_id)
                            .unwrap();
                        let packet = decoder.collect_packet().unwrap();
                        // another "ondecoded" from client.ts
                        handle_packet_from_client::<A, S, P>(
                            adapter,
                            server_state,
                            context,
                            sockets,
                            engine_connection_id,
                            packet,
                            server_event_sender,
                            responder,
                        )
                        .await;
                    }
                }
                Err(err) => {
                    // TODO: send binary attachment decode error with details
                    handle_message_processing_error(responder);
                }
            }
        }
        EnginePacketData::Empty => {
            // TODO: this is considered unknown. Ignore? or reset the buffer?
        }
    }
}

fn handle_engine_socket_close<A, S, P>(
    state: Arc<RwLock<ServerState<P::Decoder, S>>>,
    sockets: Arc<DashMap<String, Socket>>,
    engine_connection_id: String,
) where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
}

// `ondecoded` from client.ts
async fn handle_packet_from_client<A, S, P>(
    adapter: Arc<A>,
    state: Arc<RwLock<ServerState<P::Decoder, S>>>,
    context: Arc<RequestContext>,
    sockets: Arc<DashMap<String, Socket>>,
    engine_connection_id: String,
    packet: Packet,
    server_event_sender: broadcast::Sender<ServerEvent>,
    mut responder: bmrng::Responder<Option<Vec<EnginePacket>>>,
) where
    A: 'static + Adapter,
    S: 'static + Storage,
    P: 'static + Parser,
{
    match packet.packet_type {
        PacketType::Connect => {
            // CONNECT or CONNECT_ERROR packet
            let connect_packet = handle_client_connect(
                state.clone(),
                sockets,
                packet.nsp,
                engine_connection_id,
                context,
                packet.data,
            )
            .await;
            let encoded_packet = P::Encoder::encode_packet(connect_packet);
            let _ = responder.respond(Some(encoded_packet.into()));
        }
        _ => {
            dbg!("new socket.io packet from the client! {:?}", &packet);
            // TODO: Avoid using a global lock here, use DashMap
            let state = state.read().unwrap();
            if let Some(namespace) = state.namespaces.get(&packet.nsp) {
                namespace.on_packet_from_client(packet)
            } else {
                dbg!("No namespace found for nsp {:?}", &packet.nsp);
            }
            let _ = responder.respond(None);
        }
    }
}

/// Returns a packet to be sent back to the adapter/engine.io layer
async fn handle_client_connect<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    sockets: Arc<DashMap<String, Socket>>,
    namespace_name: String,
    engine_connection_id: String,
    context: Arc<RequestContext>,
    auth: PacketDataValue,
) -> Packet
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    let connect_result = if namespace_exists(state.clone(), &namespace_name) {
        Ok(connect_client_to_namespace(
            state.clone(),
            sockets,
            &namespace_name,
            &engine_connection_id,
            context.clone(),
            auth,
        )
        .await)
    } else {
        if let Some(dynamic_namespace) =
            get_matching_dynamic_namespace(state.clone(), &namespace_name, &auth).await
        {
            Ok(connect_client_to_namespace(
                state.clone(),
                sockets,
                &namespace_name,
                &engine_connection_id,
                context.clone(),
                auth,
            )
            .await)
        } else {
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

fn handle_message_processing_error(mut responder: bmrng::Responder<Option<Vec<EnginePacket>>>) {
    let _ = responder.respond(None);
}

/// socket.io/lib/client.ts :: doConnect
async fn connect_client_to_namespace<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    sockets: Arc<DashMap<String, Socket>>,
    namespace_name: &String,
    engine_connection_id: &String,
    context: Arc<RequestContext>,
    auth: PacketDataValue,
) -> (Arc<SimpleNamespace<S>>, String)
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    clear_connect_timer(state.clone(), engine_connection_id).await;
    let namespace = get_or_create_namespace(state, namespace_name).0;
    let handshake = Handshake::new(&context, auth);
    let socket_id = add_connection(sockets, namespace.clone(), handshake);
    (namespace, socket_id)
}

fn namespace_exists<D, S>(state: Arc<RwLock<ServerState<D, S>>>, name: &str) -> bool
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    state.read().unwrap().namespaces.contains_key(name)
}

/// `server._checkNamespace()` in the original implementation
async fn get_matching_dynamic_namespace<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    namespace_name: &str,
    auth: &PacketDataValue,
) -> Option<Arc<DynamicNamespace<S>>>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    let dynamic_namespaces = &state.read().unwrap().dynamic_namespaces;
    if dynamic_namespaces.is_empty() {
        return None;
    }
    let dynamic_namespace = dynamic_namespaces
        .iter()
        .find(move |(check_fn, _)| check_fn(namespace_name, auth))
        .map(|dynamic_namespace| dynamic_namespace.1.clone());

    if let Some(ref dynamic_namespace) = dynamic_namespace {
        let (simple_namespace, get_result) =
            get_or_create_namespace(state.clone(), &namespace_name);
        if get_result == GetResult::New {
            // This part is akin to `parent-namespace.ts/createChild()`
            // TODO: hook up this simple nsp to the dynamic nsp
            dynamic_namespace.connect_simple_namespace(simple_namespace);
        }
    }
    dynamic_namespace
}

fn get_or_create_namespace<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    name: &str,
) -> (Arc<SimpleNamespace<S>>, GetResult)
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    let name = if !name.starts_with('/') {
        "/".to_owned() + name
    } else {
        name.to_owned()
    };
    let mut state = state.write().unwrap();
    let namespace = state.namespaces.get(&name);
    if let Some(namespace) = namespace {
        (namespace.clone(), GetResult::Existing)
    } else {
        let namespace = Arc::new(SimpleNamespace::<S>::new(name.clone()));
        state.namespaces.insert(name, namespace.clone());
        (namespace, GetResult::New)
    }
}

/// Returns the new socket id
fn add_connection<S>(
    sockets: Arc<DashMap<String, Socket>>,
    namespace: Arc<SimpleNamespace<S>>,
    handshake: Handshake,
) -> String
where
    S: 'static + Storage,
{
    let socket = Socket::new(handshake);
    let socket_id = socket.id.clone();

    // Assuming the client is still connected. If there is a disconnect event,
    // it should be coming from the same event queue (ClientEvent)
    namespace.join_socket(&socket_id, &socket_id);

    namespace.send_event(NamespaceEvent::Connection {
        socket_id: socket_id.clone(),
    });
    sockets.insert(socket_id.clone(), socket);
    socket_id
}

fn remove_connection<S>(
    sockets: DashMap<String, Socket>,
    namespace: Arc<SimpleNamespace<S>>,
    socket_id: &str,
) where
    S: 'static + Storage,
{
    namespace.leave_socket(socket_id, socket_id);
    sockets.remove(socket_id);
    // TODO: anything else?
}

async fn setup_connect_timer(connection_id: String) {
    // TODO: complete this
    todo!();
}

async fn clear_connect_timer<D, S>(state: Arc<RwLock<ServerState<D, S>>>, connection_id: &str)
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    todo!()
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
