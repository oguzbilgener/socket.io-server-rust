use crate::adapter::Adapter;
use crate::packet::{Packet, PacketData, PacketType};
use crate::server::ServerOptions;
use crate::transport::{
    PollingTransport, PollingTransportOptions, RequestReply, Transport, TransportBase,
    TransportCreateData, TransportError, TransportEvent, TransportKind, WebsocketTransport,
    WebsocketTransportOptions,
};
use crate::util::{RequestContext, ServerError};
use serde_json::json;
use std::mem;
use std::sync::Arc;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::{broadcast, mpsc};

/// This callback type is mainly used for `ack`s for packets sent by the server.
type Callback = Box<dyn Fn() + Send + 'static>;

#[derive(Display, Debug, Clone, PartialEq)]
pub enum SocketEvent {
    Open { socket_id: String },
    Close { socket_id: String },
    Flush { socket_id: String },
    Drain { socket_id: String },
    Upgrade { socket_id: String },
    Heartbeat { socket_id: String },
    Message { socket_id: String, data: PacketData },
    Error { socket_id: String },
}

pub struct Socket<A: 'static + Adapter> {
    pub id: String,
    upgrade_state: UpgradeState,
    ready_state: ReadyState,
    remote_address: String,
    write_buffer: Vec<Packet>,
    event_sender: mpsc::Sender<SocketEvent>,
    transport_holder: TransportHolder<A>,
    /// This is the `packetsFn` from the original engine.io JS implementation
    pending_callbacks: Vec<Callback>,
    // This is the `sentCallbackFn` from the original engine.io JS implementation
    flushed_callbacks: Vec<CallbackBatch>,
}

struct TransportHolder<A: 'static + Adapter> {
    transport: Transport<A>,
    transport_event_sender: broadcast::Sender<TransportEvent>,
    // TODO: get rid of this?
    transport_event_receiver: Arc<AsyncMutex<broadcast::Receiver<TransportEvent>>>,
}

impl<A: 'static + Adapter> TransportHolder<A> {
    pub(crate) fn new(
        transport: Transport<A>,
        transport_event_sender: broadcast::Sender<TransportEvent>,
        transport_event_receiver: broadcast::Receiver<TransportEvent>,
    ) -> Self {
        let holder = TransportHolder {
            transport,
            transport_event_receiver: Arc::new(AsyncMutex::new(transport_event_receiver)),
            transport_event_sender,
        };
        holder
    }
}

enum CallbackBatch {
    NonFramed { callbacks: Vec<Callback> },
    Framed { callback: Callback },
}

pub enum SocketError {
    TransportError,
    ParseError,
}

impl<A: 'static + Adapter> Socket<A> {
    pub fn new(
        id: String,
        remote_address: String,
        event_sender: mpsc::Sender<SocketEvent>,
        transport_create_data: TransportCreateData<A::WsHandle>,
    ) -> Self {
        let (transport, transport_event_sender, transport_event_receiver) =
            Self::create_transport(transport_create_data);

        Socket {
            id,
            remote_address,
            upgrade_state: UpgradeState::Initial,
            ready_state: ReadyState::Opening,
            transport_holder: TransportHolder::new(
                transport,
                transport_event_sender,
                transport_event_receiver,
            ),
            write_buffer: Vec::new(),
            event_sender,
            // TODO: avoid the channel initiation here.
            pending_callbacks: Vec::new(),
            flushed_callbacks: Vec::new(),
        }
    }

    fn create_transport(
        transport_create_data: TransportCreateData<A::WsHandle>,
    ) -> (
        Transport<A>,
        broadcast::Sender<TransportEvent>,
        broadcast::Receiver<TransportEvent>,
    ) {
        let (transport_event_sender, transport_event_receiver) = broadcast::channel(128);
        let transport: Transport<A> = match transport_create_data {
            TransportCreateData::WebSocket {
                supports_binary,
                socket,
            } => Transport::WebSocket(A::WebSocket::new(
                WebsocketTransportOptions {
                    per_message_deflate: true,
                    supports_binary: supports_binary,
                },
                transport_event_sender.clone(),
                socket,
            )),
            TransportCreateData::Polling { jsonp } => {
                Transport::Polling(A::Polling::new(
                    PollingTransportOptions {
                        // FIXME: get these options from somewhere
                        max_http_buffer_size: 1024,
                        http_compression: None,
                        jsonp,
                    },
                    transport_event_sender.clone(),
                ))
            }
        };
        (transport, transport_event_sender, transport_event_receiver)
    }

    fn set_transport(&mut self, transport_create_data: TransportCreateData<A::WsHandle>) {
        let (transport, transport_event_sender, transport_event_receiver) =
            Self::create_transport(transport_create_data);
        self.transport_holder =
            TransportHolder::new(transport, transport_event_sender, transport_event_receiver);
    }

    pub fn get_transport(&self) -> &Transport<A> {
        &self.transport_holder.transport
    }

    pub fn get_transport_mut(&mut self) -> &mut Transport<A> {
        &mut self.transport_holder.transport
    }

    pub fn get_transport_mut_as_polling(&mut self) -> Result<&mut A::Polling, ServerError> {
        let transport = self.get_transport_mut();
        if let Transport::Polling(transport) = transport {
            Ok(transport)
        } else {
            // TODO: add error details: Not expecting a websocket transport at the moment
            Err(ServerError::Unknown)
        }
    }

    pub fn get_transport_kind(&self) -> TransportKind {
        self.transport_holder.transport.get_transport_kind()
    }

    pub async fn open(&mut self, server_options: &ServerOptions) {
        self.ready_state = ReadyState::Open;

        // Send the open packet as json string
        self.send_open_packet(server_options).await;

        self.event_sender
            .send(SocketEvent::Open {
                socket_id: self.id.clone(),
            })
            .await;
        self.set_ping_timeout();
    }

    pub fn close(&mut self, discard: bool) {
        if self.ready_state == ReadyState::Open {
            self.ready_state = ReadyState::Closing {
                with_discard: discard,
            };

            if self.write_buffer.is_empty() {
                self.transport_holder.transport.close();
            }
            // If the write buffer is not empty, the original engine.io
            // JS implementation waits for the drain event to occur
            // (in the `flush` method) to close the transport.
            // In this implementation `flush` takes care of this for us,
            // when the `readyState` is `Closing`
        }
    }

    pub(crate) async fn send_packet(&mut self, packet: Packet, callback: Option<Callback>) {
        if self.ready_state == ReadyState::Opening || self.ready_state == ReadyState::Open {
            // TODO: The original JS implementation here adds a `compress` option.

            self.write_buffer.push(packet.clone());

            // The original implementation sends a "packetCreate" event that's not
            // used anywhere.

            if let Some(callback) = callback {
                self.pending_callbacks.push(callback);
            }

            self.flush().await;
        }
    }

    pub async fn send(&mut self, packet_data: PacketData, callback: Option<Callback>) {
        self.send_packet(
            Packet {
                packet_type: PacketType::Message,
                data: packet_data,
            },
            callback,
        )
        .await;
    }

    pub async fn write(&mut self, packet_data: PacketData, callback: Option<Callback>) {
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
        &mut self,
        request_context: RequestContext,
        body: Option<A::Body>,
    ) -> Result<A::Response, ServerError> {
        let transport = self.get_transport_mut_as_polling()?;
        match transport.handle_request(&request_context, body).await? {
            RequestReply::Action(event) => {
                match event {
                    TransportEvent::Drain => {
                        if let Some(packets) = self.flush().await {
                            let transport = self.get_transport_mut_as_polling()?;
                            transport.respond_with_packets(&request_context, packets)
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

    pub fn maybe_upgrade(&mut self, transport_create_data: TransportCreateData<A::WsHandle>) {
        // TODO: lots of things here
        self.set_transport(transport_create_data);
    }

    // One method that's missing here is `clearTransport` which you can find in
    // the original engine.io JS implementation. We don't really need it.

    async fn close_transport(&mut self, discard: bool) {
        if discard {
            self.transport_holder.transport.discard();
        }
        self.transport_holder.transport.close().await;
    }

    async fn flush(&mut self) -> Option<Vec<Packet>> {
        let transport = &mut self.transport_holder.transport;
        if self.ready_state != ReadyState::Closed
            && transport.is_writable()
            && self.write_buffer.len() > 0
        {
            let id = &self.id;
            self.event_sender
                .send(SocketEvent::Flush {
                    socket_id: id.clone(),
                })
                .await;

            // Replace the write buffer with an empty one, take the ownership
            // of the full one and send it to transport
            let mut buffer = Some(mem::replace(&mut self.write_buffer, Vec::new()));
            if let Transport::Polling(_) = transport {
                // `.send` doesn't really do anything for Polling transport.
                //
                transport.send(buffer.take().unwrap()).await;
            }

            // The original engine.io JS implementation does this weird duck-typed
            // thing in `sentCallbackFn` to collect callbacks in batches when
            // it's a polling transport. This is an attempt to implement the
            // same logic in Rust.
            let callbacks = mem::replace(&mut self.pending_callbacks, Vec::new());
            let flushed_callbacks: Vec<CallbackBatch> = if transport.supports_framing() {
                callbacks
                    .into_iter()
                    .map(move |callback| CallbackBatch::Framed { callback })
                    .collect()
            } else {
                vec![CallbackBatch::NonFramed { callbacks }]
            };
            self.flushed_callbacks.extend(flushed_callbacks);

            // Send a 'drain' event to the server, which will forward it to external listeners
            let _ = self
                .event_sender
                .send(SocketEvent::Drain {
                    socket_id: id.clone(),
                })
                .await;

            if let ReadyState::Closing { with_discard } = self.ready_state {
                // Just flushed the write buffer, now we can actually close
                // the transport.
                if with_discard {
                    transport.discard();
                } else {
                    transport.close().await;
                }
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

    async fn send_open_packet(&mut self, server_options: &ServerOptions) {
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
    async fn on_close(&mut self, reason: SocketError, description: &str) {
        if self.ready_state != ReadyState::Closed {
            self.ready_state = ReadyState::Closed;

            // FIXME: clear pingTimeoutTimer
            // FIXME: clear check interval timer
            // FIXME: clear upgrade timeout timer
            self.write_buffer.clear();
            self.pending_callbacks.clear();
            self.close_transport(false).await;

            // Send a "close" event to server
            let _ = self
                .event_sender
                .send(SocketEvent::Close {
                    socket_id: self.id.clone(),
                })
                .await;
        }
    }

    async fn on_transport_error(&mut self, error: TransportError) {
        if self.ready_state == ReadyState::Opening || self.ready_state == ReadyState::Open {
            match error {
                // Used instead of the `error` type, undocumented pseudo packet in
                // the JS implementation
                TransportError::PacketParseError => {
                    self.on_close(SocketError::ParseError, "FIXME").await
                }
                _ => self.on_close(SocketError::TransportError, "FIXME").await,
            }
        }
        self.event_sender.send(SocketEvent::Error {
            socket_id: self.id.clone(),
        });
    }

    // on new packet from the transport
    async fn on_packet(&mut self, packet: Packet) {
        if self.ready_state == ReadyState::Open {
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
                    .await
                }
                PacketType::Upgrade => {
                    if self.ready_state != ReadyState::Closed
                        && self.upgrade_state == UpgradeState::Upgrading
                    {
                        self.close_transport(false).await;
                        // Emit an upgrade event
                        let _ = self
                            .event_sender
                            .send(SocketEvent::Upgrade {
                                socket_id: self.id.clone(),
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
                    let _ = self
                        .event_sender
                        .send(SocketEvent::Message {
                            socket_id: self.id.clone(),
                            data: packet.data,
                        })
                        .await;
                }
                _ => {}
            }
        }
    }

    async fn on_flush(&mut self) {
        self.flush().await;
    }

    async fn on_drain(&mut self) {
        if !self.flushed_callbacks.is_empty() {
            // Unlike the original JS implementation, we're not passing the
            // `transport` argument in the `ack` callbacks here. This is _probably_
            // fine, as this argument doesn't seem to be used anywhere.
            let first_callback_batch = self.flushed_callbacks.remove(0);
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

pub async fn subscribe_socket_to_transport_events<A: 'static + Adapter>(
    socket: Arc<AsyncMutex<Socket<A>>>,
) {
    let receiver = {
        let socket = socket.lock().await;
        socket.transport_holder.transport_event_sender.subscribe()
    };
    let subscriber_task = async move {
        let mut receiver = receiver;
        while let Ok(message) = receiver.recv().await {
            let _ = match message {
                TransportEvent::Error { error } => {
                    println!("transport error");
                    socket.lock().await.on_transport_error(error).await;
                }
                TransportEvent::Packet { packet } => {
                    println!("on packet!");
                    socket.lock().await.on_packet(packet).await;
                }
                TransportEvent::Drain => {
                    println!("on drain");
                    let mut socket = socket.lock().await;
                    socket.on_flush().await;
                    socket.on_drain().await;
                }
                TransportEvent::Close => {
                    println!("on close");
                    // TODO: fix the reason
                    socket
                        .lock()
                        .await
                        .on_close(SocketError::TransportError, "FIXME")
                        .await
                }
            };
        }
    };
    tokio::spawn(subscriber_task);
}

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
