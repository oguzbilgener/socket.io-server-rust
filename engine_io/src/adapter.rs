use crate::transport::*;

pub trait Adapter: Send + Sync {
    type Websocket: TransportImpl;
    type Polling: TransportImpl;

    fn close(&self);
    fn open_socket(&self);
    fn create_websocket_transport(&self, options: WebsocketTransportOptions) -> Self::Websocket;
    fn create_polling_transport(&self, options: PollingTransportOptions) -> Self::Polling;

    // verify (server)
    // prepare (server)
    // handleRequest (server)
    // handleUpgrade (server)
    // onWebSocket (server)
    // attach (server)
}
