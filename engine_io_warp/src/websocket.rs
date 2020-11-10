use crate::adapter_warp::{AdapterResponse, AdapterWsHandle, StandaloneAdapter};
use async_trait::async_trait;
use engine_io_parser::binary::encoder as binary_encoder;
use engine_io_parser::decoder::{decode_packet, Encoded};
use engine_io_parser::string::encoder as string_encoder;
use engine_io_server::packet::Packet;
use engine_io_server::transport::{
    TransportBase, TransportCmd, TransportEvent, WebsocketTransport,
    WebsocketTransportOptions,
};
use engine_io_server::util::RequestContext;
use futures::future::{AbortHandle, Abortable};
use futures::stream::{self, SplitSink, SplitStream};
use futures::{StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, Notify};
use warp::ws::{Message, WebSocket};

pub struct WarpWebsocketTransport {
    pub event_sender: broadcast::Sender<TransportEvent>,
    pub options: WebsocketTransportOptions,
    drop_notify: Arc<Notify>,
}

impl WebsocketTransport<StandaloneAdapter> for WarpWebsocketTransport {
    fn new(
        options: WebsocketTransportOptions,
        event_sender: broadcast::Sender<TransportEvent>,
        socket_handle: AdapterWsHandle,
        upgrade_request_context: Arc<RequestContext>,
        cmd_receiver: broadcast::Receiver<TransportCmd>,
    ) -> Self {
        let (websocket_sender, websocket_receiver) = socket_handle;
        let drop_notify = Arc::new(Notify::new());
        tokio::spawn(subscribe_to_websocket_messages(
            websocket_receiver,
            event_sender.clone(),
            upgrade_request_context,
            drop_notify.clone(),
        ));
        tokio::spawn(subscribe_to_socket_commands(
            cmd_receiver,
            drop_notify.clone(),
            websocket_sender,
            options,
        ));
        WarpWebsocketTransport {
            event_sender,
            options,
            drop_notify,
        }
    }
}

#[async_trait]
impl TransportBase<AdapterResponse> for WarpWebsocketTransport {

    fn is_writable(&self) -> bool {
        // TODO: Is this right? Do we need this anymore?
        true
    }
}

// TODO: implement Drop to close channels and stop tasks!
impl Drop for WarpWebsocketTransport {
    fn drop(&mut self) {
        self.drop_notify.notify();
    }
}

async fn subscribe_to_websocket_messages(
    mut websocket_receiver: SplitStream<WebSocket>,
    event_sender: broadcast::Sender<TransportEvent>,
    upgrade_request_context: Arc<RequestContext>,
    drop_notify: Arc<Notify>,
) {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    tokio::spawn(Abortable::new(
        async move {
            while let Some(result) = websocket_receiver.next().await {
                match result {
                    Ok(message) => {
                        // We are ignoring WebSocket ping and pong frames as engine.io has custom ping and pongs.
                        let encoded = if message.is_binary() {
                            Some(Encoded::Binary(message.as_bytes()))
                        } else if message.is_text() {
                            Some(Encoded::Text(message.to_str().unwrap()))
                        } else if message.is_close() {
                            break;
                        } else {
                            None
                        };

                        if let Some(encoded) = encoded {
                            match decode_packet(encoded) {
                                Ok(packet) => {
                                    let _ = event_sender.send(TransportEvent::Packet {
                                        context: upgrade_request_context.clone(),
                                        packet,
                                    });
                                }
                                Err(parse_err) => {
                                    // TODO: trace this error
                                    println!("Parse packet error {:?}", parse_err);
                                }
                            };
                        }
                    }
                    Err(err) => {
                        // Client disconnected
                        println!("Err received from websocket {:?}", err);
                        break;
                    }
                }
            }
            // No more messages
            let _ = event_sender.send(TransportEvent::Close);
        },
        abort_registration,
    ));
    drop_notify.notified().await;
    abort_handle.abort();
}

async fn subscribe_to_socket_commands(
    mut cmd_receiver: broadcast::Receiver<TransportCmd>,
    drop_notify: Arc<Notify>,
    mut websocket_sender: SplitSink<WebSocket, Message>,
    options: WebsocketTransportOptions,
) {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    tokio::spawn(Abortable::new(
        async move {
            while let Ok(cmd) = cmd_receiver.recv().await {
                match cmd {
                    TransportCmd::Send { packets } => {
                        if let Err(warp_error) =
                            send_packets(&mut websocket_sender, packets, options).await
                        {
                            println!(
                                "failed to send packets via warp websocket, {:?}",
                                warp_error
                            )
                            // TODO: trace this!
                        };
                    }
                }
            }
        },
        abort_registration,
    ));
    drop_notify.notified().await;
    abort_handle.abort();
}

async fn send_packets(
    websocket_sender: &mut SplitSink<WebSocket, Message>,
    packets: Vec<Packet>,
    options: WebsocketTransportOptions,
) -> Result<(), warp::Error> {
    if options.supports_binary {
        let messages = packets
            .into_iter()
            .map(|packet| Message::binary(binary_encoder::encode_packet(&packet)));
        stream::iter(messages)
            .map(Ok)
            .forward(websocket_sender)
            .await
    } else {
        let messages = packets
            .into_iter()
            .map(|packet| Message::text(string_encoder::encode_packet(&packet)));
        // TODO: deal with errors and return it
        stream::iter(messages)
            .map(Ok)
            .forward(websocket_sender)
            .await
    }
}
