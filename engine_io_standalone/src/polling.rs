use async_trait::async_trait;
use engine_io::packet::*;
use engine_io::transport::*;
use tokio::sync::mpsc::Sender;

pub struct PollingTransport {}

#[async_trait]
impl TransportImpl for PollingTransport {
    async fn open(&self) {
        unimplemented!();
    }

    async fn close(&self) {
        unimplemented!();
    }

    fn discard(&self) {
        unimplemented!();
    }

    async fn send(&self, packets: &[Packet]) {
        unimplemented!();
    }

    fn set_sid(&mut self, sid: String) {
        unimplemented!();
    }

    fn set_event_sender(&mut self, event_sender: Sender<TransportEvent>) {
        unimplemented!();
    }

    fn supports_framing(&self) -> bool {
        true
    }

    fn is_writable(&self) -> bool {
        true
    }
}
