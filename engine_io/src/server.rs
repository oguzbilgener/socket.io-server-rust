use crate::adapter::Adapter;
use crate::socket::{subscribe_socket_to_transport_events, Socket, SocketEvent};
use crate::transport::*;
use engine_io_parser::packet::{Packet, PacketData};
use futures::future::{AbortHandle, Abortable};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct ServerOptions {
    pub ping_timeout: u32,
    pub ping_interval: u32,
    pub upgrade_timeout: u32,
    pub transports: Vec<TransportKind>,
    pub allow_upgrades: bool,
    pub initial_packet: Option<Packet>,
    // node ws-specific options:
    // - maxHttpBufferSize
    // - perMessageDeflate
    // - httpCompression
    // -- cors
}

pub struct Server<A: 'static + Adapter> {
    adapter: A,
    // TODO: consider using something like https://github.com/jonhoo/flurry
    clients: Arc<RwLock<HashMap<String, Arc<Mutex<Socket<A>>>>>>,
    // ping timeout handler EngineIoSocketTimeoutHandler
    pub options: ServerOptions,
    /// Event sender to Socket instances
    socket_event_sender: Sender<SocketEvent>,
    /// Event listener for Socket instances
    socket_event_receiver: Arc<Mutex<Receiver<SocketEvent>>>,
    /// Sends `ServerEvents` to external listeners
    server_event_sender: Arc<Mutex<Sender<ServerEvent>>>,
}

impl Default for ServerOptions {
    fn default() -> ServerOptions {
        ServerOptions {
            ping_timeout: 5000,
            ping_interval: 25000,
            upgrade_timeout: 10000,
            transports: vec![TransportKind::WebSocket, TransportKind::Polling],
            allow_upgrades: true,
            initial_packet: None,
        }
    }
}

pub enum ServerError {
    UnknownTransport = 0,
    UnknownSid = 1,
    BadHandshakeMethod = 2,
    BadRequest = 3,
    Forbidden = 4,
    Unknown = -1,
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
        data: PacketData,
    },
}

impl<A: 'static + Adapter> Server<A> {
    pub fn new(adapter: A, options: ServerOptions) -> (Self, Receiver<ServerEvent>) {
        // To listen events from socket instances
        let (socket_listen_tx, socket_listen_rx) = channel(1024);
        // To send events to the owner of this Server instance
        let (server_send_tx, server_send_rx) = channel(1024);
        let server = Server {
            adapter,
            clients: Arc::new(RwLock::new(HashMap::new())),
            options,
            socket_event_sender: socket_listen_tx,
            socket_event_receiver: Arc::new(Mutex::new(socket_listen_rx)),
            server_event_sender: Arc::new(Mutex::new(server_send_tx)),
        };
        // TODO: Should probably move this to wherever the adapter starts the
        // server or attaches to a server.
        server.subscribe_to_socket_events();
        (server, server_send_rx)
    }

    pub async fn listen(&self) -> Result<(), Box<dyn Error>> {
        // TODO: handle shutdown properly by receiving a shutdown signal
        // sending it to socket instances.
        println!("listen!");
        Ok(())
    }

    pub async fn close(&self) {
        {
            let mut clients = self.clients.write().await;
            for (_id, socket) in clients.iter_mut() {
                // TODO: make this more concurrent?
                socket.lock().await.close(true).await;
            }
        }
        self.adapter.close().await;
    }

    pub fn handle_request(&self) -> Result<usize, String> {
        unimplemented!()
    }

    /// Generate a new ID for a client.
    /// Note: This generates IDs in a different format from the original JS
    /// engine.io implementation, which uses a library called
    /// [base64id](https://www.npmjs.com/package/base64id) that doesn't seem
    /// to guarantee uniqueness.
    pub fn generate_id(&self) -> String {
        Uuid::new_v4().to_hyphenated().to_string()
    }

    pub async fn handshake(
        &mut self,
        transport_kind: TransportKind,
        supports_binary: bool,
        remote_address: &str,
    ) {
        let id = self.generate_id();

        let transport: Transport<A> = match transport_kind {
            TransportKind::WebSocket => Transport::WebSocket(
                self.adapter
                    .create_websocket_transport(WebsocketTransportOptions {
                        per_message_deflate: true,
                    }),
            ),
            TransportKind::Polling => {
                Transport::Polling(
                    self.adapter
                        .create_polling_transport(PollingTransportOptions {
                            // FIXME: get these options from somewhere
                            max_http_buffer_size: 1024,
                            http_compression: None,
                            supports_binary,
                        }),
                )
            }
        };

        let socket: Arc<Mutex<Socket<A>>> = Arc::new(Mutex::new(Socket::new(
            id.clone(),
            transport,
            remote_address.to_owned(),
            self.socket_event_sender.clone(),
        )));

        {
            self.clients
                .write()
                .await
                .insert(id.clone(), socket.clone());

            let mut socket = socket.lock().await;

            socket.open(&self.options).await;

            if let Some(initial_message_packet) = self.options.initial_packet.clone() {
                socket.send_packet(initial_message_packet, None).await;
            }
        }

        subscribe_socket_to_transport_events(socket).await;

        self.adapter.set_cookie();

        // Emit a "connection" event. This is an internal event that's used by socket_io
        let _ = self
            .server_event_sender
            .clone()
            .lock()
            .await
            .send(ServerEvent::Connection {
                connection_id: id.clone(),
            })
            .await;
    }

    pub fn handle_upgrade(&self) {
        unimplemented!()
    }

    pub async fn clients_count(&self) -> usize {
        self.clients.read().await.len()
    }

    fn subscribe_to_socket_events(&self) {
        // TODO: remove the abort handle, as dropping the receiver may be sufficient to exit the loop?
        let (abort_handle, abort_registration) = AbortHandle::new_pair();

        let receiver = self.socket_event_receiver.clone();
        let external_event_sender = self.server_event_sender.clone();
        let clients = self.clients.clone();

        let subscriber_task = Abortable::new(
            async move {
                let mut receiver = receiver.lock().await;
                while let Some(message) = receiver.recv().await {
                    let _ = match message {
                        SocketEvent::Close { socket_id } => {
                            clients.write().await.remove(&socket_id);
                        }
                        SocketEvent::Flush { socket_id } => {
                            // Forward the Flush event to the external listener
                            external_event_sender
                                .lock()
                                .await
                                .send(ServerEvent::Flush {
                                    connection_id: socket_id,
                                })
                                .await;
                        }
                        SocketEvent::Drain { socket_id } => {
                            // Forward the Drain event to the external listener
                            external_event_sender
                                .lock()
                                .await
                                .send(ServerEvent::Drain {
                                    connection_id: socket_id,
                                })
                                .await;
                        }
                        SocketEvent::Message { socket_id, data } => {
                            // Forward the Drain event to the external listener
                            external_event_sender
                                .lock()
                                .await
                                .send(ServerEvent::Message {
                                    connection_id: socket_id,
                                    data: data,
                                })
                                .await;
                        }
                        _ => {}
                    };
                }
            },
            abort_registration,
        );

        tokio::spawn(subscriber_task);
    }
}

impl From<&str> for ServerError {
    fn from(message: &str) -> ServerError {
        if message == "Transport unknown" {
            ServerError::UnknownTransport
        } else if message == "Session ID unknown" {
            ServerError::UnknownSid
        } else if message == "Bad handshake method" {
            ServerError::BadHandshakeMethod
        } else if message == "Bad request" {
            ServerError::BadRequest
        } else if message == "Forbidden" {
            ServerError::Forbidden
        } else {
            ServerError::Unknown
        }
    }
}
