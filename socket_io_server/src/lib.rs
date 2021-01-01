#![forbid(unsafe_code)]
extern crate engine_io_server;

pub mod server;
pub mod storage;
pub mod namespace;
pub mod socket;
pub mod connection;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
