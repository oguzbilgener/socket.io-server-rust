use crate::transport::*;
use async_trait::async_trait;

#[async_trait]
pub trait Adapter: Send + Sync {
    type WebSocket: TransportImpl;
    type Polling: TransportImpl;

    async fn listen(&self);

    async fn close(&self);
    fn open_socket(&self);

    fn set_cookie(&self);

    fn create_websocket_transport(&self, options: WebsocketTransportOptions) -> Self::WebSocket;
    fn create_polling_transport(&self, options: PollingTransportOptions) -> Self::Polling;

    // verify (server)
    // prepare (server)
    // handleRequest (server)
    // handleUpgrade (server)
    // onWebSocket (server)
    // attach (server)
}
