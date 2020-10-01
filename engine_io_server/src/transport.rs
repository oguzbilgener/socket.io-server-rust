use crate::adapter::Adapter;
use crate::util::{RequestContext, ServerError};
use async_trait::async_trait;
use bytes::Bytes;
use engine_io_parser::packet::Packet;
use futures::stream;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::broadcast;

#[async_trait]
pub trait TransportBase<R>: 'static + Send + Sync
where
    R: 'static + Send + Sync + Sized,
{
    async fn close(&mut self);
    fn discard(&self);
    async fn send(&mut self, packets: Vec<Packet>);
    fn is_writable(&self) -> bool;
}

#[async_trait]
pub trait PollingTransport<R, B>
where
    R: 'static + Send + Sync + Sized,
    B: 'static,
{
    fn new(
        options: PollingTransportOptions,
        sender: broadcast::Sender<TransportEvent>,
    ) -> Self;

    // TODO: come up with a better error type?
    fn respond_with_packets(
        &self,
        request_context: &RequestContext,
        packets: Vec<Packet>,
    ) -> Result<R, ServerError>;

    async fn handle_request(
        &self,
        request_context: &RequestContext,
        body: Option<B>,
    ) -> RequestResult<R>;
}

#[async_trait]
pub trait WebsocketTransport<S>
where
    S: 'static,
{
    fn new(
        options: WebsocketTransportOptions,
        sender: broadcast::Sender<TransportEvent>,
        socket: S,
    ) -> Self;
}

#[derive(Debug)]
pub enum Transport<A: 'static + Adapter> {
    WebSocket(A::WebSocket),
    Polling(A::Polling),
}

impl<A: 'static + Adapter> Transport<A> {
    pub fn supports_framing(&self) -> bool {
        match self {
            Transport::WebSocket(_) => true,
            Transport::Polling(_) => false,
        }
    }
}

impl<A: 'static + Adapter> Transport<A> {
    pub(crate) fn get_transport_kind(&self) -> TransportKind {
        match self {
            Transport::WebSocket(_) => TransportKind::WebSocket,
            Transport::Polling(_) => TransportKind::Polling,
        }
    }
}

#[derive(Display, Debug, Clone, Copy, PartialEq, EnumString)]
pub enum TransportKind {
    #[strum(serialize = "websocket")]
    WebSocket,
    #[strum(serialize = "polling")]
    Polling,
}

impl TransportKind {
    pub fn parse(input: &str) -> Result<TransportKind, ServerError> {
        TransportKind::from_str(input).map_err(|_| ServerError::UnknownTransport)
    }
}

#[derive(Debug)]
pub enum TransportCreateData<S>
where
    S: 'static,
{
    // TODO: in engine.io 4.0, polling doesn't support binary
    // https://github.com/socketio/engine.io/commit/c099338e04bb5a93b8997c871c27d238455d2deb
    Polling { jsonp: bool },
    WebSocket { supports_binary: bool, socket: S },
}

#[derive(Debug, Copy, Clone)]
pub struct WebsocketTransportOptions {
    pub per_message_deflate: bool,
    pub supports_binary: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct PollingTransportOptions {
    // TODO: https://github.com/socketio/engine.io/commit/c099338e04bb5a93b8997c871c27d238455d2deb
    pub max_http_buffer_size: usize,
    pub http_compression: Option<HttpCompressionOptions>,
    pub jsonp: bool,
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

// TODO: get rid of this since engine.io 4.0 only has plaintext b64 polling response body
#[derive(Display, Debug, Clone, PartialEq)]
pub enum ResponseBodyData {
    Plaintext(String),
    Binary(Vec<u8>),
}

impl ResponseBodyData {
    pub fn into_bytes(self: ResponseBodyData) -> Vec<u8> {
        match self {
            ResponseBodyData::Plaintext(text) => text.into_bytes(),
            ResponseBodyData::Binary(binary) => binary,
        }
    }

    pub fn into_iter_stream(self: ResponseBodyData) -> stream::Iter<std::vec::IntoIter<u8>> {
        stream::iter(self.into_bytes())
    }
}

impl From<ResponseBodyData> for Bytes {
    fn from(data: ResponseBodyData) -> Self {
        Bytes::from(data.into_bytes())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RequestReply<R: 'static> {
    Action(TransportEvent),
    Response(R),
}

impl<R: 'static> From<R> for RequestReply<R> {
    fn from(response: R) -> RequestReply<R> {
        RequestReply::Response(response)
    }
}

// TODO: more diverse errors, how?
pub type RequestResult<R> = Result<RequestReply<R>, ServerError>;

pub fn get_common_polling_response_headers(
    request_context: &RequestContext,
    xhr: bool,
) -> HashMap<&str, String> {
    let mut headers: HashMap<&str, String> = HashMap::new();

    let user_agent = &request_context.user_agent;
    if user_agent.contains(";MSIE") || user_agent.contains("Trident/") {
        headers.insert("X-XSS-Protection", "0".to_owned());
    }

    if xhr {
        if let Some(origin) = &request_context.origin {
            headers.insert("Access-Control-Allow-Credentials", "true".to_owned());
            headers.insert("Access-Control-Allow_origin", origin.clone());
        } else {
            headers.insert("Access-Control-Allow-Origin", "*".to_owned());
        }
    }
    headers
}

// Kind of unfortunate that we have to implement this...
// I wonder if there's a shorter way to do it.
#[async_trait]
impl<A: 'static + Adapter> TransportBase<A::Response> for Transport<A> {
    #[inline]
    async fn close(&mut self) {
        match self {
            Transport::WebSocket(transport) => transport.close().await,
            Transport::Polling(transport) => transport.close().await,
        }
    }

    #[inline]
    fn discard(&self) {
        match self {
            Transport::WebSocket(transport) => transport.discard(),
            Transport::Polling(transport) => transport.discard(),
        }
    }

    #[inline]
    async fn send(&mut self, packets: Vec<Packet>) {
        match self {
            Transport::WebSocket(transport) => transport.send(packets).await,
            Transport::Polling(transport) => transport.send(packets).await,
        }
    }

    #[inline]
    fn is_writable(&self) -> bool {
        match self {
            Transport::WebSocket(transport) => transport.is_writable(),
            Transport::Polling(transport) => transport.is_writable(),
        }
    }
}
