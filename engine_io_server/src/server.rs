use crate::adapter::Adapter;
use crate::socket::{subscribe_socket_to_transport_events, Callback, Socket, SocketEvent};
use crate::transport::{Transport, TransportCreateData, TransportKind};
use crate::util::{HttpMethod, RequestContext, SendPacketError, ServerError, SetCookie};
use dashmap::DashMap;
use engine_io_parser::packet::{Packet, PacketData};
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

pub const BUFFER_CONST: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerOptions {
    pub ping_timeout: u32,
    pub ping_interval: u32,
    pub upgrade_timeout: u32,
    pub transports: Vec<TransportKind>,
    pub allow_upgrades: bool,
    pub initial_packet: Option<Packet>,
    // TODO: implement this
    // pub allow_request: Option<Box<dyn (Fn() -> bool) + Send + 'static>>,
    pub cookie: Option<CookieOptions>,
    // TODO: node ws-specific options:
    // - maxHttpBufferSize
    // - perMessageDeflate
    // - httpCompression
    // -- cors
    pub buffer_factor: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CookieOptions {
    pub name: String,
    pub path: String,
    pub http_only: bool,
}

#[derive(Debug, Clone)]
pub struct EventSenders {
    // Event sender to external owner
    server: broadcast::Sender<ServerEvent>,
    /// Event sender to Socket instances
    client: mpsc::Sender<SocketEvent>,
}

pub struct ServerState {
    socket_receiver_temp: Option<mpsc::Receiver<SocketEvent>>,
}

pub struct Server<A: 'static + Adapter> {
    state: Arc<RwLock<ServerState>>,
    // TODO: don't use a mutex here, instead have an internal socket state
    clients: Arc<DashMap<String, Arc<Socket<A>>>>,
    event_senders: EventSenders,
    // TODO: ping timeout handler EngineIoSocketTimeoutHandler
    pub options: ServerOptions,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            ping_timeout: 5000,
            ping_interval: 25000,
            upgrade_timeout: 10000,
            transports: vec![TransportKind::WebSocket, TransportKind::Polling],
            allow_upgrades: true,
            initial_packet: None,
            cookie: Some(CookieOptions::default()),
            // allow_request: None,
            buffer_factor: 2,
        }
    }
}

impl Default for CookieOptions {
    fn default() -> Self {
        CookieOptions {
            name: "io".to_owned(),
            path: "/".to_owned(),
            http_only: true,
        }
    }
}

#[derive(Display, Debug, Clone, PartialEq)]
pub enum ServerEvent {
    /// Socket ID
    Connection {
        connection_id: String,
    },
    Flush {
        connection_id: String,
    },
    Drain {
        connection_id: String,
    },
    Message {
        connection_id: String,
        context: Arc<RequestContext>,
        data: PacketData,
    },
    Error {
        connection_id: String,
    },
}

impl<A: 'static + Adapter> Server<A> {
    pub fn new(options: ServerOptions) -> Self {
        // To listen events from socket instances
        let (client_event_sender, client_event_receiver) =
            mpsc::channel(options.buffer_factor * BUFFER_CONST);
        // To send events to the owner of this Server instance
        let (server_event_sender, _) = broadcast::channel(options.buffer_factor * BUFFER_CONST);

        Server {
            state: Arc::new(RwLock::new(ServerState {
                socket_receiver_temp: Some(client_event_receiver),
            })),
            clients: Arc::new(DashMap::new()),
            event_senders: EventSenders {
                server: server_event_sender,
                client: client_event_sender,
            },
            options,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        // First time calling subscribe, also start listening events from `Socket` instances
        if let Some(socket_receiver_temp) = self.state.write().unwrap().socket_receiver_temp.take()
        {
            self.subscribe_to_socket_events(socket_receiver_temp);
        }

        self.event_senders.server.subscribe()
        // TODO: handle shutdown properly by receiving a shutdown signal
        // sending it to socket instances.
    }

    pub async fn close(&self) {
        // TODO: consider sending signals or dropping channels instead of closing them like this?
        // TODO: or drop the whole thing. The server, the sockets, everything.
        todo!();
        // for socket in self.clients.iter() {
        //     socket.value().close(true);
        // }
    }

    pub async fn close_socket(&self, connection_id: &str) {
        if let Some((_key, socket)) = self.clients.remove(connection_id) {
            // TODO: convert this to drop
            todo!();
            // socket.close(true);
        }
    }

    // TODO: consider converting ack callbacks into optional async Results?
    // `connection_id` is an owned string just because of a Rust compiler issue.
    pub async fn send_packet_with_ack(
        &self,
        connection_id: String,
        packet: Packet,
        callback: Option<Callback>,
    ) -> Result<(), SendPacketError> {
        match self.clients.get(&connection_id) {
            Some(client) => Ok(client.send_packet(packet, None).await),
            None => Err(SendPacketError::UnknownConnectionId),
        }
    }

    pub async fn send_packet(
        &self,
        connection_id: String,
        packet: Packet,
    ) -> Result<(), SendPacketError> {
        match self.clients.get(&connection_id) {
            Some(client) => Ok(client.send_packet(packet, None).await),
            None => Err(SendPacketError::UnknownConnectionId),
        }
    }

    pub async fn handle_request(
        &self,
        context: RequestContext,
        body: Option<A::Body>,
    ) -> Result<A::Response, ServerError> {
        let context = Arc::new(context);
        let sid_ref = context.query.get("sid");
        let sid = sid_ref.map(|s| s.to_owned());
        self.verify_request(sid_ref, false, context.transport_kind, context.http_method)
            .await?;
        if let Some(sid) = sid {
            let client = self.get_client_or_error(&sid)?;
            let response = client.handle_polling_request(context.clone(), body).await?;
            Ok(response)
        } else {
            let (sid, response) = self.handshake(context, HandshakeData::Polling).await?;
            Ok(response)
        }
    }

    /// Akin to `onWebSocket` from engine.io js
    // TODO: handle errors, socket closure etc.
    pub async fn handle_upgrade(&self, context: RequestContext, socket: A::WsHandle) {
        let context = Arc::new(context);
        let sid_ref = context.query.get("sid");
        let sid = sid_ref.map(|s| s.to_owned());

        if let Some(sid) = sid {
            // TODO: don't panic
            let client = self.get_client_or_error(&sid).expect("TODO: fix this");
            client.maybe_upgrade(context, todo!());
        // TODO: implement this!
        // let client =
        // TODO: call socket.maybe_upgrade()
        } else {
            self.handshake(context, HandshakeData::WebSocket { socket })
                .await;
            todo!();
        }
    }

    pub async fn verify_request(
        &self,
        sid: Option<&String>,
        upgrade: bool,
        transport_kind: TransportKind,
        http_method: HttpMethod,
    ) -> Result<(), ServerError> {
        if let Some(sid) = sid {
            let client = self.clients.get(sid);
            if let Some(client) = client {
                let client_transport_kind = client.get_transport_kind();
                if !upgrade && Some(transport_kind) != client_transport_kind {
                    return Err(ServerError::BadRequest);
                }
            } else {
                return Err(ServerError::UnknownSid);
            }
        } else {
            if http_method != HttpMethod::Get {
                return Err(ServerError::BadHandshakeMethod);
            }
            // FIXME: fix allow_request calls
            /*if let Some(validator) = &self.options.allow_request {
                // FIXME: pass some request parameters to this validator
                // to make it useful
                let valid = validator();
                if !valid {
                    return Err(ServerError::BadRequest);
                }
            }*/
        }
        Ok(())
    }

    /// Generate a new ID for a client.
    /// Note: This generates IDs in a different format from the original JS
    /// engine.io implementation, which uses a library called
    /// [base64id](https://www.npmjs.com/package/base64id) that doesn't seem
    /// to guarantee uniqueness.
    pub fn generate_id() -> String {
        Uuid::new_v4().to_hyphenated().to_string()
    }

    /// Returns the new client ID
    pub async fn handshake(
        &self,
        context: Arc<RequestContext>,
        data: HandshakeData<A::WsHandle>,
    ) -> Result<(String, A::Response), ServerError> {
        let sid = Self::generate_id();
        let supports_binary = !context.query.contains_key("b64");
        let jsonp = !supports_binary && !context.query.contains_key("j");

        let context = Arc::new(context.with_set_cookie(SetCookie::from_cookie_options(
            &self.options.cookie,
            sid.clone(),
        )));

        let transport_create_data = match data {
            HandshakeData::Polling => TransportCreateData::Polling { jsonp },
            HandshakeData::WebSocket { socket } => TransportCreateData::WebSocket {
                supports_binary,
                socket,
            },
        };

        let socket = Arc::new(Socket::new(
            sid.clone(),
            context.clone(),
            self.event_senders.client.clone(),
            transport_create_data,
        ));

        self.clients.insert(sid.clone(), socket.clone());

        socket.open(&self.options).await;

        // TODO: send this initial packet in the handshake request response?
        // so we'd need to return it to the adapter
        if let Some(initial_message_packet) = self.options.initial_packet.clone() {
            socket.send_packet(initial_message_packet, None).await;
        }

        subscribe_socket_to_transport_events(socket).await;

        let response = {
            let client = self.get_client_or_error(&sid)?;
            match client.get_transport_or_fail()?.as_ref() {
                Transport::Polling(_) => Ok(client.handle_polling_request(context, None).await?),
                _ => Err(ServerError::BadRequest),
            }
        };

        // Emit a "connection" event. This is an internal event that's used by socket_io
        let _ = self
            .event_senders
            .server
            .clone()
            .send(ServerEvent::Connection {
                connection_id: sid.clone(),
            });
        response.map(|response| Ok((sid, response)))?
    }

    pub fn clients_count(&self) -> usize {
        self.clients.len()
    }

    pub fn get_client_or_error(&self, id: &str) -> Result<Arc<Socket<A>>, ServerError> {
        if let Some(client) = self.clients.get(id) {
            Ok(client.value().clone())
        } else {
            Err(ServerError::UnknownSid)
        }
    }

    fn subscribe_to_socket_events(&self, client_event_receiver: mpsc::Receiver<SocketEvent>) {
        let server_event_sender = self.event_senders.server.clone();
        let clients = self.clients.clone();

        tokio::spawn(async move {
            let mut receiver = client_event_receiver;
            while let Some(message) = receiver.recv().await {
                match message {
                    SocketEvent::Close { socket_id } => {
                        clients.remove(&socket_id);
                    }
                    SocketEvent::Flush { socket_id } => {
                        // Forward the Flush event to the external listener
                        let _ = server_event_sender.send(ServerEvent::Flush {
                            connection_id: socket_id,
                        });
                    }
                    SocketEvent::Drain { socket_id } => {
                        // Forward the Drain event to the external listener
                        let _ = server_event_sender.send(ServerEvent::Drain {
                            connection_id: socket_id,
                        });
                    }
                    SocketEvent::Message {
                        socket_id,
                        context,
                        data,
                    } => {
                        // Forward the Drain event to the external listener
                        let _ = server_event_sender.send(ServerEvent::Message {
                            connection_id: socket_id,
                            context,
                            data,
                        });
                    }
                    SocketEvent::Error { socket_id } => {
                        let _ = server_event_sender.send(ServerEvent::Error {
                            connection_id: socket_id,
                        });
                    }
                    _ => {}
                }
            }
        });
    }

    fn subscribe_to_commands(&self) {
        // TODO: receive packet send requests using a MPSC listener ?
        todo!();
    }
}

#[derive(Debug)]
pub enum HandshakeData<S>
where
    S: 'static,
{
    Polling,
    WebSocket { socket: S },
}
