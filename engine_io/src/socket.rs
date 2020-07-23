use crate::adapter::Adapter;
use crate::transport::*;
use engine_io_parser::packet::*;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex;

type Callback = Box<dyn FnOnce()>;

#[derive(Display, Debug, Clone, PartialEq)]
pub enum SocketEvent {
    Open { socket_id: String },
    Close { socket_id: String },
    Flush { socket_id: String },
    Drain { socket_id: String },
    Upgrade { socket_id: String },
    Heartbeat { socket_id: String },
}

pub struct Socket<A, W, P>
where
    A: 'static + Adapter<W, P>,
    W: 'static + TransportImpl,
    P: 'static + TransportImpl,
{
    pub id: String,
    upgrade_state: UpgradeState,
    ready_state: ReadyState,
    remote_address: String,
    transport: Transport<W, P>,
    adapter: &'static A,
    write_buffer: Vec<Packet>,
    event_sender: Sender<SocketEvent>,
    transport_event_receiver: Arc<Mutex<Receiver<TransportEvent>>>,
}

pub enum SocketError {
    TransportError,
    ParseError,
}

macro_rules! get_transport {
    ($ref:expr) => {{
        let transport: Box<&mut dyn TransportImpl> = match &mut $ref.transport {
            Transport::Websocket(transport) => Box::new(transport),
            Transport::Polling(transport) => Box::new(transport),
        };
        transport
    }};
}

impl<A, W, P> Socket<A, W, P>
where
    A: 'static + Adapter<W, P>,
    W: 'static + TransportImpl,
    P: 'static + TransportImpl,
{
    pub fn new(
        id: String,
        transport: Transport<W, P>,
        remote_address: String,
        adapter: &'static A,
        initial_packet: Option<Packet>,
        event_sender: Sender<SocketEvent>,
    ) -> Self {
        let (transport_event_tx, transport_event_rx) = channel(128);
        let mut socket = Socket {
            id,
            remote_address,
            upgrade_state: UpgradeState::Initial,
            ready_state: ReadyState::Opening,
            transport,
            adapter,
            write_buffer: Vec::new(),
            event_sender,
            transport_event_receiver: Arc::new(Mutex::new(transport_event_rx)),
        };
        get_transport!(socket).set_event_sender(transport_event_tx);
        socket.subscribe_to_transport_events();
        socket.open();
        if let Some(initial_message_packet) = initial_packet {
            socket.send_packet(&initial_message_packet, None);
        }
        socket
    }

    fn set_transport(&mut self, transport: Transport<W, P>) {
        self.transport = transport;
        let transport = get_transport!(self);
        let (transport_event_tx, transport_event_rx) = channel(128);
        transport.set_event_sender(transport_event_tx);

        self.subscribe_to_transport_events();
    }

    fn close_transport(&mut self) {
        get_transport!(self).close();
        // TODO: do a few other things
    }

    pub fn open(&mut self) {
        self.ready_state = ReadyState::Open;
        let transport = get_transport!(self);
        transport.set_sid(self.id.clone());

        // Send the open packet as json string
        self.send_open_packet();

        // TODO: await this send call
        self.event_sender.send(SocketEvent::Open {
            socket_id: self.id.clone(),
        });
        self.set_ping_timeout();
    }

    pub fn send_packet(&mut self, packet: &Packet, callback: Option<Callback>) {
        if self.ready_state != ReadyState::Closing && self.ready_state != ReadyState::Closed {
            // The original JS implementation here adds a `compress` option. Do we need it?

            self.write_buffer.push(packet.clone());

            self.flush();
        }
    }

    pub fn send(&mut self, packet_data: PacketData, callback: Option<Callback>) {
        self.send_packet(
            &Packet {
                packet_type: PacketType::Message,
                data: packet_data,
            },
            callback,
        );
    }

    pub fn write(&mut self, packet_data: PacketData, callback: Option<Callback>) {
        self.send_packet(
            &Packet {
                packet_type: PacketType::Message,
                data: packet_data,
            },
            callback,
        );
    }

    pub fn maybe_upgrade(&'static mut self, transport: W) {
        // TODO: lots of things here
        self.set_transport(Transport::Websocket(transport));
    }

    fn flush(&mut self) {
        let transport = get_transport!(self);
        if self.ready_state != ReadyState::Closed
            && transport.is_writable()
            && self.write_buffer.len() > 0
        {
            let id = &self.id;
            // TODO: The JS engine.io server emits a "flush" event here
            self.event_sender.send(SocketEvent::Flush {
                socket_id: id.clone(),
            });

            transport.send(&self.write_buffer);

            self.write_buffer.truncate(0);

            // TODO: The JS engine.io socket emits a "drain" event here.
            self.event_sender.send(SocketEvent::Drain {
                socket_id: id.clone(),
            });
        }
    }

    fn get_available_upgrades(&self) -> Vec<&str> {
        unimplemented!();
    }

    fn set_ping_timeout(&self) {
        unimplemented!();
    }

    fn send_open_packet(&mut self) {
        let open_packet_data = json!({
            "sid": self.id,
            "upgrades": self.get_available_upgrades(),
            "ping_interval": 0,// self.server_options.ping_interval,
            "ping_timeout": 0, //self.server_options.ping_timeout
        });
        let open_packet = Packet {
            packet_type: PacketType::Open,
            data: PacketData::from(open_packet_data.to_string()),
        };
        self.send_packet(&open_packet, None);
    }

    fn subscribe_to_transport_events(&mut self) {
        let receiver = self.transport_event_receiver.clone();
        let subscriber_task = async move {
            let mut receiver = receiver.lock().await;
            while let Some(message) = receiver.recv().await {
                let _ = match message {
                    TransportEvent::Error => {
                        println!("transport error");
                    }
                    TransportEvent::Packet { packet } => {
                        println!("on packet!");
                    }
                    TransportEvent::Drain => {
                        println!("on drain");
                    }
                    TransportEvent::Close => {
                        println!("on close");
                    }
                };
            }
        };
        tokio::spawn(subscriber_task);
        // tokio::spawn(async {});
        /*
        this.transport.once('error', onError);
        this.transport.on('packet', onPacket);
        this.transport.on('drain', flush);
        this.transport.once('close', onClose);
        */
    }

    async fn on_close(&mut self, reason: SocketError, description: &str) {
        if self.ready_state != ReadyState::Closed {
            self.ready_state = ReadyState::Closed;

            // FIXME: clear pingTimeoutTimer
            // FIXME: clear check interval timer
            // FIXME: clear upgrade timeout timer
            self.write_buffer.clear();
            self.close_transport();

            // Send a "close" event to listeners.
            let _ = self
                .event_sender
                .send(SocketEvent::Close {
                    socket_id: self.id.clone(),
                })
                .await;
        }
    }

    async fn on_error(&mut self) {
        self.on_close(SocketError::TransportError, "FIXME").await;
    }

    // Used instead of the `error` type, undocumented pseudo packet in the JS implementation
    async fn on_packet_parse_error(&mut self) {
        // FIXME: use this
        self.on_close(SocketError::ParseError, "FIXME").await;
    }

    // on new packet from the transport
    async fn on_packet(&mut self, packet: Packet) {
        if self.ready_state == ReadyState::Open {
            // TODO: self.emit("packet");

            self.set_ping_timeout();

            match packet.packet_type {
                PacketType::Ping => self.send_packet(
                    &Packet {
                        packet_type: PacketType::Pong,
                        data: PacketData::Empty,
                    },
                    None,
                ),
                PacketType::Upgrade => {
                    if self.ready_state != ReadyState::Closed
                        && self.upgrade_state == UpgradeState::Upgrading
                    {
                        self.close_transport();
                        // Emit an upgrade event
                        let _ = self
                            .event_sender
                            .send(SocketEvent::Upgrade {
                                socket_id: self.id.clone(),
                            })
                            .await;
                        self.set_ping_timeout();
                        self.flush();
                    }
                }
                // PacketType::Error => {}
                PacketType::Message => {
                    // TODO: self.emit("data", packet.data);
                    // TODO: self.emit("message", packet.data);
                }
                _ => {}
            }
        }
    }
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
    Closing,
    Closed,
}
