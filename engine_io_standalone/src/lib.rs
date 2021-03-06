#![forbid(unsafe_code)]
extern crate warp;

pub use engine_io_warp::polling;
pub use engine_io_warp::adapter_warp as standalone;
pub use engine_io_warp::websocket;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
