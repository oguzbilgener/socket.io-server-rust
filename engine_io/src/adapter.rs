use crate::transport::*;

pub trait Adapter<W: TransportImpl, P: TransportImpl>: Send + Sync {
    fn close(&self);
    fn open_socket(&self);
    fn create_websocket_transport(&self, options: WebsocketTransportOptions) -> Transport<W, P>;
    fn create_polling_transport(&self, options: PollingTransportOptions) -> Transport<W, P>;
}
