use crate::namespace::Namespace;
use crate::storage::Storage;
use core::default::Default;
use engine_io_server::adapter::{Adapter, ListenOptions};
use engine_io_server::server::BUFFER_CONST;
use engine_io_server::server::{ServerEvent as EngineEvent, ServerOptions as EngineOptions};
use regex::Regex;
use serde_json::json;
use socket_io_parser::decoder::Decoder;
use socket_io_parser::engine_io_parser::EnginePacketData;
use socket_io_parser::packet::PacketParseError;
use socket_io_parser::packet::{Packet, PacketDataValue, PacketType, ProtoPacket};
use socket_io_parser::parser::Parser;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;
use std::sync::{RwLock, RwLockReadGuard};
use tokio::sync::{broadcast, mpsc};

pub struct ServerState<D, S>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    namespaces: HashMap<String, Namespace<S>>,
    parent_namespaces: HashMap<String, Regex>,
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
            parent_namespaces: HashMap::new(),
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
    /// Socket ID
    Connect {
        connection_id: String,
        nsp: String,
        data: PacketDataValue,
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
            event_sender,
        }
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        let event_receiver_temp = self.state.write().unwrap().event_receiver_temp.take();
        if let Some(event_receiver) = event_receiver_temp {
            let client_event_sender = self.subscribe_to_client_events().await;
            self.subscribe_to_engine_events(client_event_sender).await;
            event_receiver
        } else {
            self.event_sender.subscribe()
        }
    }

    async fn subscribe_to_engine_events(
        &self,
        client_event_sender: mpsc::UnboundedSender<ClientEvent>,
    ) {
        let mut event_listener = self.adapter.subscribe().await;
        let state = self.state.clone();
        let event_sender = self.event_sender.clone();
        let adapter = self.adapter.clone();

        tokio::spawn(async move {
            while let Ok(message) = event_listener.recv().await {
                let _ = match message {
                    // socket.io/lib/index.ts :: onconnection
                    EngineEvent::Connection { connection_id } => {
                        println!("new engine connection from {}!", connection_id);
                        tokio::spawn(setup_connect_timer(connection_id));
                    }
                    // socket.io/lib/client.ts :: ondata
                    EngineEvent::Message {
                        connection_id,
                        data,
                    } => {
                        tokio::spawn(handle_new_message(
                            connection_id,
                            data,
                            state.clone(),
                            event_sender.clone(),
                            client_event_sender.clone(),
                        ));
                    }
                    // socket.io/lib/client.ts :: onerror
                    EngineEvent::Error { connection_id } => {
                        // TODO: provide more details about the error
                        adapter.close_socket(&connection_id).await;
                    }
                    _ => {
                        // TODO: what to do with other events?
                    }
                };
            }
        });
    }

    async fn subscribe_to_client_events(&self) -> mpsc::UnboundedSender<ClientEvent> {
        let (event_sender, mut event_receiver) =
            tokio::sync::mpsc::unbounded_channel::<ClientEvent>();
        let state = self.state.clone();

        tokio::spawn(async move {
            while let Some(message) = event_receiver.recv().await {
                let _ = match message {
                    ClientEvent::Connect {
                        connection_id,
                        nsp: namespace,
                        data,
                    } => {
                        handle_client_connect(state.clone(), namespace, connection_id, data)
                        // TODO: somehow implement client.js :: doConnect and server.js :: _checkNamespace
                    }
                    ClientEvent::Error => {
                        todo!();
                    }
                };
            }
        });
        event_sender
    }
}

pub struct HandleEngineMessageError {
    reason: &'static str,
}

impl From<PacketParseError> for HandleEngineMessageError {
    fn from(_err: PacketParseError) -> Self {
        HandleEngineMessageError {
            reason: "Could not parse text packet",
        }
    }
}

pub async fn handle_new_message<D, S>(
    connection_id: String,
    data: EnginePacketData,
    state: Arc<RwLock<ServerState<D, S>>>,
    server_event_sender: broadcast::Sender<ServerEvent>,
    client_event_sender: mpsc::UnboundedSender<ClientEvent>,
) where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    println!("new message from {}", connection_id);
    println!("> {:?}", data);

    // TODO: Get a decoder or create a new one, decode the header,
    // determine if there are attachments (i.e. decode_header returns an enum)
    // If text type:
    // - if there are no attachments, we're done.
    // - if there are attachments, put this decoder into a hashmap for this connection_id
    // If binary type:
    // - check if there is decoder in the hashmap. if there is, send this binary to decoder. if there isn't return error
    // - check if decoder is done receiving all the attachments.
    //   when it is done, get the final Packet

    match data {
        EnginePacketData::Text(text) => {
            // FIXME: don't unwrap
            let proto_packet = match D::decode_proto_packet(text) {
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
                ProtoPacket::Multipart { header } => {
                    if header.has_attachments() {
                        let mut state = state.write().unwrap();
                        if state.decoders_in_progress.contains_key(&connection_id) {
                            // TODO: emit an error?
                            // TODO: handle send error
                            let _ = client_event_sender.send(ClientEvent::Error);
                            let _ = server_event_sender.send(ServerEvent::Error {
                                message: "sequence error".to_owned(),
                            });
                        } else {
                            state
                                .decoders_in_progress
                                .insert(connection_id, D::new(header));
                        }
                    } else {
                        // TODO: Ignore this? binary_event or binary_ack packet with no attachment.
                    }
                }
                ProtoPacket::Plain { packet } => {
                    // "ondecoded" from client.ts
                    handle_packet_from_client(
                        connection_id,
                        packet,
                        state,
                        server_event_sender,
                        client_event_sender,
                    );
                }
            }
        }
        EnginePacketData::Binary(buffer) => {
            let server_state = state.clone();
            let mut state = state.write().unwrap();
            if let Some(decoder) = state.decoders_in_progress.get_mut(&connection_id) {
                match decoder.decode_binary_data(buffer) {
                    Ok(done) => {
                        if done {
                            let mut decoder =
                                state.decoders_in_progress.remove(&connection_id).unwrap();
                            let packet = decoder.collect_packet();
                            drop(state);
                            // another "ondecoded" from client.ts
                            handle_packet_from_client(
                                connection_id,
                                packet,
                                server_state,
                                server_event_sender,
                                client_event_sender,
                            );
                        }
                    }
                    Err(err) => {
                        // TODO: send binary attachment decode error with details
                        let _ = client_event_sender.send(ClientEvent::Error);
                    }
                };
            } else {
                // This is an attachment that has nowhere to go.
                // TODO: propagate an error maybe. "unexpected attachment"?
            }
        }
        EnginePacketData::Empty => {
            // TODO: this is considered unknown. Ignore? or reset the buffer?
        }
    }
}

// `ondecoded` from client.ts
fn handle_packet_from_client<D, S>(
    connection_id: String,
    packet: Packet,
    state: Arc<RwLock<ServerState<D, S>>>,
    server_event_sender: broadcast::Sender<ServerEvent>,
    client_event_sender: mpsc::UnboundedSender<ClientEvent>,
) where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    match packet.packet_type {
        PacketType::Connect => {
            // TODO: handle send error
            let _ = client_event_sender.send(ClientEvent::Connect {
                connection_id: connection_id.clone(),
                nsp: packet.nsp,
                data: packet.data,
            });
        }
        _ => {
            // TODO: find the namespace for the socket and send `_onpacket`. from client.ts
        }
    }
}

// TODO: Make the auth parameter generic
async fn handle_client_connect<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    namespace_name: String,
    connection_id: String,
    auth: PacketDataValue,
) -> Result<(), Packet>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    let can_connect = if !state
        .read()
        .unwrap()
        .namespaces
        .contains_key(&namespace_name)
    {
        check_namespace(state.clone(), &auth).await
    } else {
        true
    };

    if !can_connect {
        return Err(Packet {
            packet_type: PacketType::ConnectError,
            nsp: namespace_name,
            data: json!({
                "message": "Invalid namespace"
            })
            .into(),
            id: None,
        });
    }

    // socket.io/lib/client.ts :: doConnect
    clear_connect_timer(state.clone(), &connection_id).await;

    let namespace = get_namespace(state.clone(), NamespaceDescriptor::Text(namespace_name)).await;
    if let Some(namespace) = namespace {
        namespace.add_connection(connection_id, auth);
    } else {
        // TODO: return an error somehow?
    }

    Ok(())
}

async fn check_namespace<D, S>(state: Arc<RwLock<ServerState<D, S>>>, auth: &PacketDataValue) -> bool
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    todo!();
}

#[derive(Debug, Clone)]
pub enum NamespaceDescriptor {
    Text(String),
    Regex(Regex),
}

// The `of` method
async fn get_namespace<D, S>(
    state: Arc<RwLock<ServerState<D, S>>>,
    name: NamespaceDescriptor,
) -> Option<Namespace<S>>
where
    D: 'static + Decoder,
    S: 'static + Storage,
{
    todo!();
}

async fn setup_connect_timer(connection_id: String) {
    // TODO: complete this
    todo!();
}

async fn clear_connect_timer<D, S>(state: Arc<RwLock<ServerState<D, S>>>, connection_id: &str)
where
    D: 'static + Decoder,
    S: 'static + Storage {
        todo!()
    }
