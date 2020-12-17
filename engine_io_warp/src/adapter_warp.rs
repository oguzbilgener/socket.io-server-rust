use crate::polling::*;
use crate::websocket::*;
use async_trait::async_trait;
use bytes::Bytes;
use engine_io_server::adapter::{Adapter, ListenOptions};
use engine_io_server::server::{Server, ServerEvent, ServerOptions};
use engine_io_server::packet::Packet;
use engine_io_server::transport::{TransportEvent, TransportKind};
use engine_io_server::util::{HttpMethod, RequestContext, ServerError};
use futures::stream::{SplitSink, SplitStream};
use futures::{FutureExt, StreamExt};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use warp::http::header::{CONTENT_TYPE, ORIGIN, USER_AGENT};
use warp::http::HeaderMap;
use warp::http::Response;
use warp::hyper::Body;
use warp::path::{self, FullPath};
use warp::reject::Reject;
use warp::reply::Reply;
use warp::ws::{Message, WebSocket, Ws};
use warp::{Filter, Rejection};

#[derive(Clone)]
pub struct WarpAdapterOptions {
    pub socket_addr: SocketAddr,
    pub buffer_factor: usize,
    pub request_body_content_limit: u64,
}

impl Default for WarpAdapterOptions {
    fn default() -> Self {
        WarpAdapterOptions {
            socket_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            buffer_factor: 32usize,
            request_body_content_limit: 1024 * 1024 * 16,
        }
    }
}

pub struct StandaloneAdapter {
    server: Arc<Server<Self>>,
    options: WarpAdapterOptions,
}

#[derive(Debug)]
pub struct WarpError {
    error: Box<ServerError>,
}

pub type AdapterResponse = Response<Body>;
pub type AdapterWsHandle = (SplitSink<WebSocket, Message>, SplitStream<WebSocket>);

impl Reject for WarpError {}

async fn on_ws_upgrade(
    socket: WebSocket,
    server: Arc<Server<StandaloneAdapter>>,
    context: RequestContext,
) {
    server.handle_upgrade(context, socket.split()).await;
}

async fn on_poll_request(
    server: Arc<Server<StandaloneAdapter>>,
    context: RequestContext,
) -> Result<AdapterResponse, Rejection> {
    match server.handle_request(context, None).await {
        Ok(response) => Ok::<AdapterResponse, Rejection>(response),
        Err(error) => Ok(server_error_to_warp_result(error)),
    }
}

async fn on_data_request(
    body: Bytes,
    server: Arc<Server<StandaloneAdapter>>,
    context: RequestContext,
) -> Result<AdapterResponse, Rejection> {
    match server.handle_request(context, Some(body)).await {
        Ok(response) => Ok(response),
        Err(error) => Ok(server_error_to_warp_result(error)),
    }
}

fn headers_into_map(headers: HeaderMap) -> HashMap<String, String> {
    todo!();
}

fn with_server(
    server: Arc<Server<StandaloneAdapter>>,
) -> impl Filter<Extract = (Arc<Server<StandaloneAdapter>>,), Error = Infallible> + Clone {
    warp::any().map(move || server.clone())
}

fn build_context(
    http_method: HttpMethod,
) -> impl Filter<Extract = (RequestContext,), Error = Rejection> + Copy + Clone {
    warp::any()
        .and(warp::header::headers_cloned())
        .and(warp::path::full())
        .and(warp::query::<HashMap<String, String>>())
        .and(warp::header::header(USER_AGENT.as_str()))
        .and(warp::header::header(ORIGIN.as_str()))
        .and(warp::header::header(CONTENT_TYPE.as_str()))
        .and(warp::addr::remote())
        .and_then(
            move |headers: HeaderMap,
                  path: warp::path::FullPath,
                  query: HashMap<String, String>,
                  user_agent: String,
                  origin: String,
                  content_type: String,
                  address: Option<SocketAddr>| async move {
                let transport_name: &str = query.get("transport").map_or("polling", String::as_str);
                match TransportKind::parse(transport_name) {
                    Ok(transport_kind) => Ok(RequestContext {
                        headers: headers_into_map(headers),
                        query,
                        origin: Some(origin),
                        secure: false,
                        user_agent,
                        content_type,
                        transport_kind,
                        http_method,
                        remote_address: address.unwrap().to_string(),
                        // TODO: Fix this!!!
                        request_url: path.as_str().to_owned(),
                        set_cookie: None,
                    }),
                    Err(server_error) => Err(warp::reject::custom(WarpError {
                        error: Box::new(server_error),
                    })),
                }
            },
        )
}

fn server_error_to_warp_result(error: ServerError) -> AdapterResponse {
    warp::reply::html(Into::<&str>::into(error)).into_response()
    // FIXME: what's the original error response format in engine.io?
    // FIXME: come up with a wrapper struct in the same format as socket.io
    // FIXME: set the correct response status code. maybe use Response::builder() from the http crate
}

#[async_trait]
impl Adapter for StandaloneAdapter {
    type WebSocket = WarpWebsocketTransport;
    type Polling = WarpPollingTransport;
    type Options = WarpAdapterOptions;
    type Response = AdapterResponse;
    type Body = Bytes;
    type WsHandle = AdapterWsHandle;

    fn new(server_options: ServerOptions, options: Self::Options) -> Self {
        let server = Server::<Self>::new(server_options);
        StandaloneAdapter {
            options,
            server: Arc::new(server),
        }
    }

    async fn listen(&self, options: ListenOptions) -> std::io::Result<()> {
        // TODO: implement a path parser so warp's stupid path DSL doesn't bother us
        // TODO: Use `handle_preflight_request`
        // TODO: Use `destroy_upgrade_timeout`

        let websocket = path_from_str(options.path)
            .and(warp::ws())
            .and(with_server(self.server.clone()))
            .and(build_context(HttpMethod::Get))
            .map(
                move |_path: String, ws: Ws, server: Arc<Server<Self>>, context: RequestContext| {
                    ws.on_upgrade(move |socket: WebSocket| on_ws_upgrade(socket, server, context))
                },
            );

        let poll_endpoint = path_from_str(options.path)
            .and(warp::get())
            .and(with_server(self.server.clone()))
            .and(build_context(HttpMethod::Get))
            .and_then(
                |_path: String, server: Arc<Server<Self>>, context: RequestContext| {
                    on_poll_request(server, context)
                },
            );
        let data_endpoint = path_from_str(options.path)
            .and(warp::post())
            .and(warp::body::content_length_limit(
                self.options.request_body_content_limit,
            ))
            .and(warp::body::bytes())
            .and(with_server(self.server.clone()))
            .and(build_context(HttpMethod::Post))
            .and_then(
                |_path: String, body: Bytes, server: Arc<Server<Self>>, context: RequestContext| {
                    on_data_request(body, server, context)
                },
            );

        let transports = &self.server.options.transports;
        let has_ws = transports
            .iter()
            .any(|t| t.clone() == TransportKind::WebSocket);

        let web_routes = poll_endpoint.or(data_endpoint);

        // Disable websocket if transports does not include `websocket`
        if has_ws {
            // The order is important, as the ws middleware has a filter that looks
            // for an upgrade header
            let routes = websocket.or(web_routes);
            warp::serve(routes).run(self.options.socket_addr).await;
        } else {
            warp::serve(web_routes).run(self.options.socket_addr).await;
        };

        Ok(())
    }

    fn subscribe(&self) -> bmrng::RequestReceiver<ServerEvent, Option<Packet>> {
        self.server.subscribe()
    }

    async fn close(&self) {
        // TODO: this should be a drop instead...
        todo!();
    }

    async fn close_socket(&self, connection_id: &str) {
        self.server.close_socket(connection_id).await;
    }
}

// A very lazy path filter (Because warp doesn't have one)
fn path_from_str(
    scheme: &'static str,
) -> impl Filter<Extract = (String,), Error = Rejection> + Copy {
    warp::path("/")
        .and(warp::path::full())
        .and_then(move |path: FullPath| async move {
            let full_path = path.as_str();
            let scheme = scheme.split('&').collect::<Vec<&str>>()[0];
            if !full_path.starts_with(scheme) {
                return Err(warp::reject::not_found());
            }
            Ok(full_path.to_owned())
        })
}
