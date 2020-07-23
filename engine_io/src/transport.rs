use engine_io_parser::packet::*;
use tokio::sync::mpsc::Sender;
// use std::str::FromStr;
// use strum;
// use strum_macros::EnumString;

pub trait TransportImpl: Send + Sync {
    fn open(&self);
    fn close(&self);
    fn void(&self);
    fn send(&self, packets: &[Packet]);
    fn set_sid(&mut self, sid: String);
    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>);

    fn is_writable(&self) -> bool;
}

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
pub enum TransportEvent {
    Error,
    Packet { packet: Packet },
    Drain,
    Close,
}
