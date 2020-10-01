use crate::server::{ServerEvent, ServerOptions};
use crate::transport::{PollingTransport, TransportBase, WebsocketTransport};
use async_trait::async_trait;
use tokio::sync::broadcast;

pub struct ListenOptions {
    pub path: &'static str,
    pub handle_preflight_request: Option<Box<dyn Fn() + Send + 'static>>,
    // destroyUpgrade and destroyUpgradeTimeout from the JS engine.io implementation.
    pub destroy_upgrade_timeout: Option<u32>,
}

impl Default for ListenOptions {
    fn default() -> ListenOptions {
        ListenOptions {
            path: "/engine.io",
            handle_preflight_request: None,
            destroy_upgrade_timeout: Some(1000),
        }
    }
}

#[async_trait]
pub trait Adapter: 'static + Send + Sync + Sized {
    type WebSocket: TransportBase<Self::Response> + WebsocketTransport<Self::WsHandle>;
    type Polling: TransportBase<Self::Response> + PollingTransport<Self::Response, Self::Body>;
    type Options: 'static + Default + Clone;
    type Response: 'static + Send + Sync + Sized;
    type Body: 'static + Send + Sync + Sized;
    type WsHandle: 'static + Send + Sync;

    fn new(server_options: ServerOptions, options: Self::Options) -> Self;

    async fn listen(&self, options: ListenOptions) -> std::io::Result<()>;
    async fn subscribe(&self) -> broadcast::Receiver<ServerEvent>;
    async fn close(&self);
    async fn close_socket(&self, connection_id: &str);
}
