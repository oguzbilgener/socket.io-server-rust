use crate::adapter::*;
use async_trait::async_trait;
use engine_io_parser::packet::*;
use tokio::sync::mpsc::Sender;
// use std::str::FromStr;
// use strum;
// use strum_macros::EnumString;

#[async_trait]
pub trait TransportImpl: Send + Sync {
    async fn open(&self);
    async fn close(&self);
    fn discard(&self);

    async fn send(&self, packets: &[Packet]);
    fn set_sid(&mut self, sid: String);
    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>);

    fn is_writable(&self) -> bool;
    fn supports_framing(&self) -> bool;
}

#[derive(Debug)]
pub enum Transport<A: 'static + Adapter> {
    WebSocket(A::WebSocket),
    Polling(A::Polling),
}

// Kind of unfortunate that we have to implement this...
// I wonder if there's a shorter way to do it.
#[async_trait]
impl<A: 'static + Adapter> TransportImpl for Transport<A> {
    async fn open(&self) {
        match self {
            Transport::WebSocket(transport) => transport.open().await,
            Transport::Polling(transport) => transport.open().await,
        }
    }

    async fn close(&self) {
        match self {
            Transport::WebSocket(transport) => transport.close().await,
            Transport::Polling(transport) => transport.close().await,
        }
    }

    fn discard(&self) {
        match self {
            Transport::WebSocket(transport) => transport.discard(),
            Transport::Polling(transport) => transport.discard(),
        }
    }

    async fn send(&self, packets: &[Packet]) {
        match self {
            Transport::WebSocket(transport) => transport.send(packets).await,
            Transport::Polling(transport) => transport.send(packets).await,
        }
    }

    fn set_sid(&mut self, sid: String) {
        match self {
            Transport::WebSocket(transport) => transport.set_sid(sid),
            Transport::Polling(transport) => transport.set_sid(sid),
        }
    }

    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>) {
        match self {
            Transport::WebSocket(transport) => transport.set_event_sender(event_sender),
            Transport::Polling(transport) => transport.set_event_sender(event_sender),
        }
    }

    fn supports_framing(&self) -> bool {
        match self {
            Transport::WebSocket(transport) => transport.supports_framing(),
            Transport::Polling(transport) => transport.supports_framing(),
        }
    }

    fn is_writable(&self) -> bool {
        match self {
            Transport::WebSocket(transport) => transport.is_writable(),
            Transport::Polling(transport) => transport.is_writable(),
        }
    }
}

#[derive(Display, Debug)]
pub enum TransportKind {
    #[strum(serialize = "websocket")]
    WebSocket,
    #[strum(serialize = "polling")]
    Polling,
}

#[derive(Debug, Copy, Clone)]
pub struct WebsocketTransportOptions {
    pub per_message_deflate: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct PollingTransportOptions {
    pub max_http_buffer_size: usize,
    pub supports_binary: bool,
    pub http_compression: Option<HttpCompressionOptions>,
}

#[derive(Debug, Copy, Clone)]
pub struct HttpCompressionOptions {
    pub threshold: usize,
}

#[derive(Display, Debug, Clone, PartialEq)]
pub enum TransportError {
    PacketParseError,
    OtherError,
}

#[derive(Display, Debug, Clone, PartialEq)]
pub enum TransportEvent {
    Error { error: TransportError },
    Packet { packet: Packet },
    Drain,
    Close,
}
