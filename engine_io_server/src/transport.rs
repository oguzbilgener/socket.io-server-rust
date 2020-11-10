use crate::adapter::Adapter;
use crate::util::{RequestContext, ServerError};
use async_trait::async_trait;
use bytes::Bytes;
use engine_io_parser::packet::Packet;
use futures::stream;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc};
use tokio::sync::broadcast;

#[async_trait]
pub trait TransportBase<R>: 'static + Send + Sync
where
    R: 'static + Send + Sync + Sized,
{
    fn is_writable(&self) -> bool;
}

#[async_trait]
pub trait PollingTransport<A>
where
    A: 'static + Adapter,
{
    fn new(
        options: PollingTransportOptions,
        sender: broadcast::Sender<TransportEvent>,
        receiver: broadcast::Receiver<TransportCmd>,
    ) -> Self;

    // TODO: come up with a better error type?
    fn respond_with_packets(
        &self,
        request_context: Arc<RequestContext>,
        packets: Vec<Packet>,
    ) -> Result<A::Response, ServerError>;

    // TODO: can we make this non async?
    async fn handle_request(
        &self,
        request_context: Arc<RequestContext>,
        body: Option<A::Body>,
    ) -> RequestResult<A::Response>;
}

#[async_trait]
pub trait WebsocketTransport<A>
where
    A: 'static + Adapter,
{
    fn new(
        options: WebsocketTransportOptions,
        sender: broadcast::Sender<TransportEvent>,
        socket: A::WsHandle,
        upgrade_request_context: Arc<RequestContext>,
        receiver: broadcast::Receiver<TransportCmd>,
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

    pub fn as_polling_or_fail(&self) -> Result<&A::Polling, ServerError> {
        match self {
            Transport::WebSocket(_) => Err(ServerError::BadRequest),
            Transport::Polling(polling) => Ok(polling),
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
pub enum TransportCmd {
    // We have no close or discard methods / events to the current transport.
    // Dropping a transport means closing it and killing its sub tasks
    Send { packets: Vec<Packet> },
}

/// Events that are sent from the Transport back to the Socket
#[derive(Display, Debug, Clone, PartialEq)]
pub enum TransportEvent {
    Error {
        error: TransportError,
    },
    Packet {
        context: Arc<RequestContext>,
        packet: Packet,
    },
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
    request_context: Arc<RequestContext>,
    xhr: bool,
) -> HashMap<&'static str, String> {
    let mut headers: HashMap<&'static str, String> = HashMap::new();

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

#[async_trait]
impl<A: 'static + Adapter> TransportBase<A::Response> for Transport<A> {
    #[inline]
    fn is_writable(&self) -> bool {
        match self {
            Transport::WebSocket(transport) => transport.is_writable(),
            Transport::Polling(transport) => transport.is_writable(),
        }
    }
}

impl<A> Transport<A>
where
    A: 'static + Adapter,
{
    pub(crate) fn create(
        context: Arc<RequestContext>,
        transport_create_data: TransportCreateData<A::WsHandle>,
        transport_cmd_receiver: broadcast::Receiver<TransportCmd>,
    ) -> Transport<A> {
        // TODO: make this customizable
        let (transport_event_sender, _) = broadcast::channel(128);

        match transport_create_data {
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
                context,
                transport_cmd_receiver,
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
                    transport_cmd_receiver,
                ))
            }
        }
    }
}
