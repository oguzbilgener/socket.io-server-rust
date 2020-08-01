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

    async fn send(&self, packets: &[Packet]);
    fn set_sid(&mut self, sid: String);
    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>);

    fn is_writable(&self) -> bool;
    fn supports_framing(&self) -> bool;
}

#[derive(Debug)]
pub enum Transport<A: 'static + Adapter> {
    Websocket(A::Websocket),
    Polling(A::Polling),
}

#[async_trait]
impl<A: 'static + Adapter> TransportImpl for Transport<A> {
    async fn open(&self) {
        match self {
            Transport::Websocket(transport) => transport.open().await,
            Transport::Polling(transport) => transport.open().await,
        }
    }

    async fn close(&self) {
        match self {
            Transport::Websocket(transport) => transport.close().await,
            Transport::Polling(transport) => transport.close().await,
        }
    }

    async fn send(&self, packets: &[Packet]) {
        match self {
            Transport::Websocket(transport) => transport.send(packets).await,
            Transport::Polling(transport) => transport.send(packets).await,
        }
    }

    fn set_sid(&mut self, sid: String) {
        match self {
            Transport::Websocket(transport) => transport.set_sid(sid),
            Transport::Polling(transport) => transport.set_sid(sid),
        }
    }

    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>) {
        match self {
            Transport::Websocket(transport) => transport.set_event_sender(event_sender),
            Transport::Polling(transport) => transport.set_event_sender(event_sender),
        }
    }

    fn supports_framing(&self) -> bool {
        match self {
            Transport::Websocket(transport) => transport.supports_framing(),
            Transport::Polling(transport) => transport.supports_framing(),
        }
    }

    fn is_writable(&self) -> bool {
        match self {
            Transport::Websocket(transport) => transport.is_writable(),
            Transport::Polling(transport) => transport.is_writable(),
        }
    }
}

#[derive(Display, Debug)]
pub enum TransportKind {
    #[strum(serialize = "websocket")]
    Websocket,
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
