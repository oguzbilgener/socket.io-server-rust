#![forbid(unsafe_code)]
extern crate engine_io;

use engine_io::transport::*;

pub struct WebsocketTransport {}

impl TransportImpl for WebsocketTransport {

}


struct StandaloneAdapter {

}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
