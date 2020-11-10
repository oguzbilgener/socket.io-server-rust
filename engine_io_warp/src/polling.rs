use crate::adapter_warp::{AdapterResponse, StandaloneAdapter};
use async_trait::async_trait;
use bytes::Bytes;
use engine_io_parser::binary::decoder as binary_decoder;
use engine_io_parser::string::decoder as string_decoder;
use engine_io_parser::string::encoder as string_encoder;
use engine_io_server::packet::Packet;
use engine_io_server::transport::{
    get_common_polling_response_headers, PollingTransport, PollingTransportOptions, RequestReply,
    RequestResult, ResponseBodyData, Transport, TransportBase, TransportCmd, TransportEvent,
};
use engine_io_server::util::{HttpMethod, RequestContext, ServerError};
use std::cell::RefCell;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::sync::broadcast;
use warp::http::header::{HeaderValue, ACCESS_CONTROL_ALLOW_HEADERS, CONTENT_TYPE, SET_COOKIE};
use warp::http::Response;
use warp::hyper::Body;

pub struct WarpPollingTransport {
    pub mode: Mode,
    pub event_sender: broadcast::Sender<TransportEvent>,
    pub writable: bool,
    pub options: PollingTransportOptions,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Mode {
    Xhr,
    Jsonp,
}

#[async_trait]
impl TransportBase<AdapterResponse> for WarpPollingTransport {
    /*
    async fn close(&mut self) {
        self.event_sender.send(TransportEvent::Close);
    }

    fn discard(&self) {
        unimplemented!();
    }

    async fn send(&mut self, packets: Vec<Packet>) {
        // TODO: handle the shouldClose callback business
    }
    */

    fn is_writable(&self) -> bool {
        // For simplicity, unlike the original engine.io JS implementation,
        // Polling transport is never writable. Whenever there is a poll HTTP
        // request, we just drain the whole packet buffer.
        false
    }
}

#[async_trait]
impl PollingTransport<StandaloneAdapter> for WarpPollingTransport {
    fn new(
        options: PollingTransportOptions,
        event_sender: broadcast::Sender<TransportEvent>,
        transport_cmd_receiver: broadcast::Receiver<TransportCmd>,
    ) -> Self {
        let mode = if options.jsonp {
            Mode::Jsonp
        } else {
            Mode::Xhr
        };
        WarpPollingTransport {
            mode,
            writable: false,
            event_sender,
            options,
        }
        // TODO: start the event listener task
    }

    fn respond_with_packets(
        &self,
        request_context: Arc<RequestContext>,
        packets: Vec<Packet>,
    ) -> Result<AdapterResponse, ServerError> {
        // TODO: JSONP
        let content = ResponseBodyData::Plaintext(string_encoder::encode_payload(&packets));

        respond(
            content,
            request_context,
            self.mode,
            self.options.http_compression.is_some(),
        )
    }

    async fn handle_request(
        &self,
        request_context: Arc<RequestContext>,
        body: Option<Bytes>,
    ) -> RequestResult<AdapterResponse> {
        let mode = self.mode;
        let event_sender = self.event_sender.clone();
        match (request_context.http_method, mode) {
            (HttpMethod::Get, _) => self.handle_poll_request().await,
            (HttpMethod::Post, _) => {
                Self::handle_data_request(event_sender, request_context, body).await
            }
            (HttpMethod::Options, Mode::Xhr) => Self::handle_options_request(),
            (_, _) => Self::respond_with_status_code_only(500),
        }
    }
}

// TODO: implement Drop to close channels and stop tasks!

impl WarpPollingTransport {
    async fn handle_poll_request(&self) -> RequestResult<AdapterResponse> {
        // TODO: handle premature close??
        // Not sending an async `Drain` event, but returning a sync one.
        // This will cause the caller (Socket<A>) to do a flush, and then call `respond_with_packets`.
        Ok(RequestReply::Action(TransportEvent::Drain))
    }
    async fn handle_data_request(
        event_sender: broadcast::Sender<TransportEvent>,
        request_context: Arc<RequestContext>,
        body: Option<Bytes>,
    ) -> RequestResult<AdapterResponse> {
        let is_binary = request_context.content_type == "application/octet-stream";

        let body = body.ok_or_else(|| ServerError::BadRequest)?;
        // TODO: stream the request body on Adapter

        let packets = if is_binary {
            binary_decoder::decode_payload(&body)
        } else {
            let body = std::str::from_utf8(&body).map_err(|_| ServerError::BadRequest)?;
            string_decoder::decode_payload(&body)
        }?;

        // TODO: should we send a clone of the request context here?
        // So that the socket.io-side can build the Handshake struct
        packets.into_iter().for_each(|packet| {
            // TODO: fail on Err or trace it
            let _ = event_sender.send(TransportEvent::Packet {
                context: request_context.clone(),
                packet,
            });
        });

        // text/html is required instead of text/plain to avoid an
        // unwanted download dialog on certain user-agents (GH-43)

        Ok(Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "text/html")
            .body("ok".into())
            .unwrap()
            .into())
    }

    fn handle_options_request() -> RequestResult<AdapterResponse> {
        Ok(Response::builder()
            .status(200)
            .header(ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type")
            .body(Body::empty())
            .unwrap()
            .into())
    }

    fn respond_with_status_code_only(status_code: u16) -> RequestResult<AdapterResponse> {
        Ok(Response::builder()
            .status(status_code)
            .body(Body::empty())
            .unwrap()
            .into())
    }
}

fn respond(
    content: ResponseBodyData,
    context: Arc<RequestContext>,
    mode: Mode,
    compress: bool,
) -> Result<AdapterResponse, ServerError> {
    let content_type = match &content {
        ResponseBodyData::Plaintext(_) => "text/plain; charset=UTF-8",
        ResponseBodyData::Binary(_) => "application/octet-stream",
    };
    // TODO: compression
    // TODO: Content-Encoding header from encryption
    // TODO: X-XSS-Protection header internet explorer

    let common_headers = get_common_polling_response_headers(context.clone(), mode == Mode::Xhr);

    let content: Bytes = content.into();
    let content = Body::from(content);

    let builder = Response::builder().status(200);

    let mut builder = common_headers
        .iter()
        .fold(builder, |builder, kv| builder.header(kv.0.clone(), kv.1));

    let headers = builder.headers_mut().unwrap();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));

    if let Some(set_cookie) = &context.set_cookie {
        let cookie_str: String = set_cookie.clone().into();
        headers.insert(SET_COOKIE, HeaderValue::try_from(cookie_str).unwrap());
    }

    builder.body(content).map_err(|_| ServerError::Unknown)
}
