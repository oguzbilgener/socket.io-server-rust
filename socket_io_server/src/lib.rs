#![forbid(unsafe_code)]
#![warn(clippy::all)]
extern crate engine_io_server;

pub mod server;
pub mod storage;
pub mod namespace;
pub mod socket;
pub mod connection;
pub mod error;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
