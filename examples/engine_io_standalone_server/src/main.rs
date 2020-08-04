use engine_io::server::{Server, ServerEvent, ServerOptions};
use engine_io_standalone::standalone::StandaloneAdapter;
use std::net::SocketAddr;
use tokio::prelude::*;

#[tokio::main(core_threads = 1)]
async fn main() -> io::Result<()> {
    println!("Hello, world!");

    let adapter = StandaloneAdapter::new(SocketAddr::from(([127, 0, 0, 1], 3000)));
    let server_options = ServerOptions::default();
    let (server, event_listener) = Server::new(adapter, server_options);
    let mut event_listener = event_listener;

    tokio::spawn(async move {
        while let Some(message) = event_listener.recv().await {
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
        res = server.listen() => {
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
