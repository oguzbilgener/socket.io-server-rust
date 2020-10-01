use engine_io_server::adapter::{Adapter, ListenOptions};
use engine_io_server::server::{ServerEvent, ServerOptions};
use engine_io_standalone::standalone::StandaloneAdapter;
use engine_io_standalone::standalone::WarpAdapterOptions;
use std::net::SocketAddr;
use tokio::prelude::*;

#[tokio::main(core_threads = 4)]
async fn main() -> io::Result<()> {
    println!("Starting standalone adapter!");

    let server_options = ServerOptions::default();
    let adapter_options = WarpAdapterOptions {
        socket_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
        buffer_factor: 32usize,
        request_body_content_limit: 1024 * 1024 * 16,
    };
    let server = StandaloneAdapter::new(server_options, adapter_options);
    let mut event_listener = server.subscribe().await;

    tokio::spawn(async move {
        while let Ok(message) = event_listener.recv().await {
            let _ = match message {
                ServerEvent::Connection { connection_id } => {
                    println!("new connection from {}!", connection_id);
                }
                ServerEvent::Message {
                    connection_id,
                    data,
                } => {
                    println!("new message from {}", connection_id);
                    println!("> {:?}", data);
                }
                _ => {}
            };
        }
    });

    tokio::select! {
        res = server.listen(ListenOptions::default()) => {
            if let Err(err) = res {
                panic!("failed to listen: {}", err);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("shutting down!");
        }
    }

    println!("bye!");
    Ok(())
}
