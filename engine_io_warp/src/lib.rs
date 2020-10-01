#![forbid(unsafe_code)]
extern crate engine_io_server;
extern crate engine_io_parser;
extern crate warp;
extern crate serde;

pub mod polling;
pub mod adapter_warp;
pub mod websocket;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
