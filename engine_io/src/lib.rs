#![forbid(unsafe_code)]
extern crate engine_io_parser;
extern crate serde_json;
extern crate strum;
extern crate uuid;
#[macro_use]
extern crate strum_macros;

pub mod adapter;
pub mod server;
pub mod socket;
pub mod transport;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
