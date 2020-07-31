use async_trait::async_trait;
use engine_io_parser::packet::*;
use tokio::sync::mpsc::Sender;
use enum_dispatch::enum_dispatch;
// use std::str::FromStr;
// use strum;
// use strum_macros::EnumString;

#[async_trait]
#[enum_dispatch]
pub trait TransportImpl: Send + Sync {
    async fn open(&self);
    async fn close(&self);

    async fn send(&self, packets: &[Packet]);
    fn set_sid(&mut self, sid: String);
    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>);

    fn is_writable(&self) -> bool;
    fn supports_framing(&self) -> bool;
}

// struct WebsocketX {

// }

// impl TransportImpl for WebsocketX {}

// struct PollingX {

// }

// impl TransportImpl for PollingX {}

// #[enum_dispatch(TransportImpl)]
// pub enum Stuff<W, P> {
//     WebsocketX(W),
//     PollingX(P)
// }

// pub enum AnyTransport<W: TransportImpl, P: TransportImpl> {
//     Websocket(W),
//     Polling(P),
// }

#[derive(Debug)]
pub enum Transport<W: TransportImpl, P: TransportImpl> {
    Websocket(W),
    Polling(P),
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
