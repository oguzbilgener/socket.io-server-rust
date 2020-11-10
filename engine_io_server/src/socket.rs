use crate::adapter::Adapter;
use crate::packet::{Packet, PacketData, PacketType};
use crate::server::ServerOptions;
use crate::transport::{
    PollingTransport, PollingTransportOptions, RequestReply, Transport, TransportBase,
    TransportCmd, TransportCreateData, TransportError, TransportEvent, TransportKind,
    WebsocketTransport, WebsocketTransportOptions,
};
use crate::util::{RequestContext, ServerError};
use serde_json::json;
use std::mem;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use tokio::sync::{broadcast, mpsc};

/// This callback type is mainly used for `ack`s for packets sent by the server.
pub type Callback = Box<dyn Fn() + Sync + Send + 'static>;

/// Every socket connection in Socket.io v2.x starts with polling, then upgrades to
/// websocket. Note that this will be reverse in v3.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpgradeState {
    Initial,
    Upgrading,
    Upgraded,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReadyState {
    Opening,
    Open,
    Closing { with_discard: bool },
    Closed,
}

#[derive(Display, Debug, Clone, PartialEq)]
pub enum SocketEvent {
    Open {
        socket_id: String,
    },
    Close {
        socket_id: String,
    },
    Flush {
        socket_id: String,
    },
    Drain {
        socket_id: String,
    },
    Upgrade {
        socket_id: String,
        context: Arc<RequestContext>,
    },
    Heartbeat {
        socket_id: String,
    },
    Message {
        socket_id: String,
        context: Arc<RequestContext>,
        data: PacketData,
    },
    Error {
        socket_id: String,
    },
}

pub struct SocketState<A: 'static + Adapter> {
    upgrade: UpgradeState,
    ready: ReadyState,
    transport: Option<Arc<Transport<A>>>,
    write_buffer: Vec<Packet>,
    /// This is the `packetsFn` from the original engine.io JS implementation
    pending_callbacks: Vec<Callback>,
    // This is the `sentCallbackFn` from the original engine.io JS implementation
    flushed_callbacks: Vec<CallbackBatch>,
}

pub struct Senders {
    socket: mpsc::Sender<SocketEvent>,
    // This is the command sender from Socket to Transport
    transport_cmd: broadcast::Sender<TransportCmd>,
    // This one is passed down to transports
    transport: broadcast::Sender<TransportEvent>,
}

pub struct Socket<A: 'static + Adapter> {
    pub id: String,
    pub remote_address: String,
    state: RwLock<SocketState<A>>,
    senders: Senders,
}

enum CallbackBatch {
    NonFramed { callbacks: Vec<Callback> },
    Framed { callback: Callback },
}

pub enum SocketError {
    TransportError,
    ParseError,
}

impl<A> Socket<A>
where
    A: 'static + Adapter,
{
    pub fn new(
        id: String,
        context: Arc<RequestContext>,
        event_sender: mpsc::Sender<SocketEvent>,
        transport_create_data: TransportCreateData<A::WsHandle>,
    ) -> Self {
        // TODO: make this customizable
        let (transport_cmd_sender, transport_cmd_receiver) = broadcast::channel(128);
        let (transport_event_sender, _) = broadcast::channel(128);
        let senders = Senders {
            socket: event_sender,
            transport_cmd: transport_cmd_sender,
            transport: transport_event_sender,
        };

        let remote_address = context.remote_address.clone();
        let transport = Arc::new(Transport::create(
            context,
            transport_create_data,
            transport_cmd_receiver,
        ));

        let state = SocketState {
            upgrade: UpgradeState::Initial,
            ready: ReadyState::Opening,
            transport: Some(transport),
            write_buffer: Vec::new(),
            pending_callbacks: Vec::new(),
            flushed_callbacks: Vec::new(),
        };

        Socket {
            id,
            remote_address,
            senders,
            state: RwLock::new(state),
        }
    }

    #[inline]
    fn set_transport(
        &self,
        context: Arc<RequestContext>,
        transport_create_data: TransportCreateData<A::WsHandle>,
    ) {
        let mut state = self.state.write().unwrap();
        state.transport = Some(Arc::new(Transport::create(
            context,
            transport_create_data,
            self.senders.transport_cmd.subscribe(),
        )));
    }

    #[inline]
    fn close_transport(&self) {
        drop_transport(&mut self.state.write().unwrap());
    }

    pub fn get_transport_kind(&self) -> Option<TransportKind> {
        self.state
            .read()
            .unwrap()
            .transport
            .as_ref()
            .map(|t| t.get_transport_kind())
    }

    #[inline]
    pub fn get_transport_or_fail(&self) -> Result<Arc<Transport<A>>, ServerError> {
        match self.state.read().unwrap().transport.as_ref() {
            Some(transport) => Ok(transport.clone()),
            None => Err(ServerError::Unknown),
        }
    }

    pub async fn open(&self, server_options: &ServerOptions) {
        self.state.write().unwrap().ready = ReadyState::Open;

        // Send the open packet as json string
        self.send_open_packet(server_options).await;

        let mut event_sender = self.senders.socket.clone();

        event_sender
            .send(SocketEvent::Open {
                socket_id: self.id.clone(),
            })
            .await;
        self.set_ping_timeout();
    }

    pub fn close(&self, discard: bool) {
        let state = self.state.write().unwrap();
        if state.ready == ReadyState::Open {
            let mut state = self.state.write().unwrap();
            state.ready = ReadyState::Closing {
                with_discard: discard,
            };

            if state.write_buffer.is_empty() {
                drop_transport(&mut state);
            }
            // If the write buffer is not empty, the original engine.io
            // JS implementation waits for the drain event to occur
            // (in the `flush` method) to close the transport.
            // In this implementation `flush` takes care of this for us,
            // when the `readyState` is `Closing`
        }
    }

    pub(crate) async fn send_packet(&self, packet: Packet, callback: Option<Callback>) {
        let ready = self.state.read().unwrap().ready;
        if ready == ReadyState::Opening || ready == ReadyState::Open {
            {
                let mut state = self.state.write().unwrap();
                // TODO: The original JS implementation here adds a `compress` option.

                state.write_buffer.push(packet.clone());

                // The original implementation sends a "packetCreate" event that's not
                // used anywhere.

                if let Some(callback) = callback {
                    state.pending_callbacks.push(callback);
                }
            }

            self.flush().await;
        }
    }

    pub async fn send(&self, packet_data: PacketData, callback: Option<Callback>) {
        self.send_packet(
            Packet {
                packet_type: PacketType::Message,
                data: packet_data,
            },
            callback,
        )
        .await;
    }

    pub async fn write(&self, packet_data: PacketData, callback: Option<Callback>) {
        self.send_packet(
            Packet {
                packet_type: PacketType::Message,
                data: packet_data,
            },
            callback,
        )
        .await;
    }

    pub async fn handle_polling_request(
        &self,
        request_context: Arc<RequestContext>,
        body: Option<A::Body>,
    ) -> Result<A::Response, ServerError> {
        let transport = self.get_transport_or_fail()?;
        let transport = transport.as_polling_or_fail()?;
        match transport
            .handle_request(request_context.clone(), body)
            .await?
        {
            RequestReply::Action(event) => {
                match event {
                    TransportEvent::Drain => {
                        if let Some(packets) = self.flush().await {
                            transport.respond_with_packets(request_context, packets)
                        } else {
                            // TODO: details!s
                            Err(ServerError::BadRequest)
                        }
                    }
                    _ => {
                        // TODO: what?
                        Err(ServerError::Unknown)
                    }
                }
            }
            RequestReply::Response(response) => Ok(response),
        }
    }

    pub fn maybe_upgrade(
        &self,
        context: Arc<RequestContext>,
        transport_create_data: TransportCreateData<A::WsHandle>,
    ) {
        // TODO: lots of things here
        self.set_transport(context, transport_create_data);
    }

    async fn flush(&self) -> Option<Vec<Packet>> {
        let (ready, write_buffer_length) = {
            let state = self.state.read().unwrap();
            (state.ready, state.write_buffer.len())
        };

        let mut event_sender = self.senders.socket.clone();
        if ready != ReadyState::Closed
            && write_buffer_length > 0
            && self
                .get_transport_or_fail()
                .map_or(false, |t| t.is_writable())
        {
            let id = &self.id;
            event_sender
                .send(SocketEvent::Flush {
                    socket_id: id.clone(),
                })
                .await;

            // Replace the write buffer with an empty one, take the ownership
            // of the full one and send it to transport
            let mut buffer = {
                let mut state = self.state.write().unwrap();
                Some(mem::replace(&mut state.write_buffer, Vec::new()))
            };
            self.senders.transport_cmd.send(TransportCmd::Send {
                packets: buffer.take().unwrap(),
            });

            // The original engine.io JS implementation does this weird duck-typed
            // thing in `sentCallbackFn` to collect callbacks in batches when
            // it's a polling transport. This is an attempt to implement the
            // same logic in Rust.
            let callbacks = {
                let mut state = self.state.write().unwrap();
                mem::replace(&mut state.pending_callbacks, Vec::new())
            };
            let supports_framing = self
                .get_transport_or_fail()
                .map_or(false, |t| t.supports_framing());
            let flushed_callbacks: Vec<CallbackBatch> = if supports_framing {
                callbacks
                    .into_iter()
                    .map(move |callback| CallbackBatch::Framed { callback })
                    .collect()
            } else {
                vec![CallbackBatch::NonFramed { callbacks }]
            };
            self.state
                .write()
                .unwrap()
                .flushed_callbacks
                .extend(flushed_callbacks);

            // Send a 'drain' event to the server, which will forward it to external listeners
            let _ = event_sender
                .send(SocketEvent::Drain {
                    socket_id: id.clone(),
                })
                .await;

            if let ReadyState::Closing { with_discard } = self.state.read().unwrap().ready {
                // Just flushed the write buffer, now we can actually close
                // the transport.
                self.close_transport();
            }

            if let Some(buffer) = buffer {
                return Some(buffer);
            }
        }
        None
    }

    pub fn get_available_upgrades(&self) -> Vec<&str> {
        unimplemented!();
    }

    fn set_ping_timeout(&self) {
        // TODO: set a timer
        unimplemented!();
    }

    async fn send_open_packet(&self, server_options: &ServerOptions) {
        let open_packet_data = json!({
            "sid": self.id,
            "upgrades": self.get_available_upgrades(),
            "ping_interval": server_options.ping_interval,
            "ping_timeout": server_options.ping_timeout
        });
        let open_packet = Packet {
            packet_type: PacketType::Open,
            data: PacketData::from(open_packet_data.to_string()),
        };
        self.send_packet(open_packet, None).await;
    }

    /// Called upon transport considered closed.
    async fn on_close(&self, reason: SocketError, description: &str) {
        let ready = self.state.read().unwrap().ready;
        let mut event_sender = self.senders.socket.clone();
        let socket_id = self.id.clone();
        if ready != ReadyState::Closed {
            {
                let mut state = self.state.write().unwrap();
                state.ready = ReadyState::Closed;
                // FIXME: clear pingTimeoutTimer
                // FIXME: clear check interval timer
                // FIXME: clear upgrade timeout timer
                state.write_buffer.clear();
                state.pending_callbacks.clear();
            }

            self.close_transport();

            // Send a "close" event to server
            let _ = event_sender.send(SocketEvent::Close { socket_id }).await;
        }
    }

    async fn on_transport_error(&self, error: TransportError) {
        let ready = self.state.read().unwrap().ready;
        let mut event_sender = self.senders.socket.clone();
        if ready == ReadyState::Opening || ready == ReadyState::Open {
            match error {
                // Used instead of the `error` type, undocumented pseudo packet in
                // the JS implementation
                TransportError::PacketParseError => {
                    self.on_close(SocketError::ParseError, "FIXME").await
                }
                _ => self.on_close(SocketError::TransportError, "FIXME").await,
            }
        }
        event_sender.send(SocketEvent::Error {
            socket_id: self.id.clone(),
        });
    }

    // on new packet from the transport
    async fn on_packet(&self, context: Arc<RequestContext>, packet: Packet) {
        let (ready, upgrade) = {
            let state = self.state.read().unwrap();
            (state.ready, state.upgrade)
        };
        let mut event_sender = self.senders.socket.clone();
        if ready == ReadyState::Open {
            // TODO: the original implementation sends a "packet" event here
            // but it goes unused. Maybe it's not worth cloning the packet?

            self.set_ping_timeout();

            match packet.packet_type {
                PacketType::Ping => {
                    self.send_packet(
                        Packet {
                            packet_type: PacketType::Pong,
                            data: PacketData::Empty,
                        },
                        None,
                    )
                    .await;
                    event_sender
                        .send(SocketEvent::Heartbeat {
                            socket_id: self.id.clone(),
                        })
                        .await;
                }
                PacketType::Upgrade => {
                    if ready != ReadyState::Closed && upgrade == UpgradeState::Upgrading {
                        self.close_transport();
                        // Emit an upgrade event
                        let _ = event_sender
                            .send(SocketEvent::Upgrade {
                                socket_id: self.id.clone(),
                                context,
                            })
                            .await;
                        self.set_ping_timeout();
                        self.flush().await;
                    }
                }
                PacketType::Message => {
                    // The original implementation also emits a "data" event here
                    // with the same packet data reference, but since we can't just
                    // pass around the data without cloning in a thread-safe manner,
                    // this is just a waste and it seems like only the `message` event
                    // is used.
                    let _ = event_sender
                        .send(SocketEvent::Message {
                            context,
                            socket_id: self.id.clone(),
                            data: packet.data,
                        })
                        .await;
                }
                _ => {
                    todo!();
                }
            }
        }
    }

    async fn on_flush(&self) {
        self.flush().await;
    }

    async fn on_drain(&self) {
        let mut state = self.state.write().unwrap();
        if !state.flushed_callbacks.is_empty() {
            // Unlike the original JS implementation, we're not passing the
            // `transport` argument in the `ack` callbacks here. This is _probably_
            // fine, as this argument doesn't seem to be used anywhere.
            let first_callback_batch = state.flushed_callbacks.remove(0);
            match first_callback_batch {
                CallbackBatch::Framed { callback } => {
                    // executing send callback
                    callback();
                }
                CallbackBatch::NonFramed { callbacks } => {
                    // executing batch send callback
                    callbacks.iter().for_each(|callback| {
                        callback();
                    })
                }
            }
        }
    }
}

pub async fn subscribe_socket_to_transport_events<A: 'static + Adapter>(socket: Arc<Socket<A>>) {
    let receiver = socket.senders.transport.subscribe();
    let socket = socket.clone();
    let subscriber_task = async move {
        let mut receiver = receiver;
        while let Ok(message) = receiver.recv().await {
            let _ = match message {
                TransportEvent::Error { error } => {
                    println!("transport error");
                    socket.on_transport_error(error).await;
                }
                TransportEvent::Packet { context, packet } => {
                    println!("on packet!");
                    socket.on_packet(context, packet).await;
                }
                TransportEvent::Drain => {
                    println!("on drain");
                    socket.on_flush().await;
                    socket.on_drain().await;
                }
                TransportEvent::Close => {
                    println!("on close");
                    // TODO: fix the reason
                    socket.on_close(SocketError::TransportError, "FIXME").await
                }
            };
        }
    };
    tokio::spawn(subscriber_task);
}

fn drop_transport<A>(state: &mut SocketState<A>)
where
    A: 'static + Adapter,
{
    state.transport = None
}
