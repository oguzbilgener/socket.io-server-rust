#![forbid(unsafe_code)]
extern crate engine_io;

pub mod standalone;
pub mod polling;
pub mod websocket;

use engine_io::transport::*;


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
