use crate::polling::*;
use crate::websocket::*;
use async_trait::async_trait;
use engine_io::adapter::*;
use engine_io::transport::*;
use std::net::SocketAddr;

pub struct StandaloneAdapter {
    socket_addr: SocketAddr,
}

impl StandaloneAdapter {
    pub fn new(socket_addr: SocketAddr) -> Self {
        StandaloneAdapter { socket_addr }
    }
}

#[async_trait]
impl Adapter for StandaloneAdapter {
    type WebSocket = WebsocketTransport;
    type Polling = PollingTransport;

    async fn listen(&self) {
        println!("hello!");
    }

    async fn close(&self) {
        unimplemented!();
    }

    fn set_cookie(&self) {
        // TODO: headers['Set-Cookie'] from the original implementation
        unimplemented!();
    }

    fn open_socket(&self) {
        unimplemented!();
    }

    fn create_websocket_transport(&self, options: WebsocketTransportOptions) -> Self::WebSocket {
        unimplemented!();
    }

    fn create_polling_transport(&self, options: PollingTransportOptions) -> Self::Polling {
        unimplemented!();
    }
}
