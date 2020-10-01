use crate::adapter_warp::{AdapterResponse, AdapterWsHandle};
use async_trait::async_trait;
use engine_io_parser::binary::encoder as binary_encoder;
use engine_io_parser::decoder::{decode_packet, Encoded};
use engine_io_parser::string::encoder as string_encoder;
use engine_io_server::packet::Packet;
use engine_io_server::transport::{
    RequestResult, TransportBase, TransportEvent, WebsocketTransport, WebsocketTransportOptions,
};
use engine_io_server::util::RequestContext;
use futures::stream::{self, SplitSink, SplitStream};
use futures::{FutureExt, SinkExt, StreamExt};
use tokio::sync::broadcast;
use warp::ws::{Message, WebSocket};

pub struct WarpWebsocketTransport {
    pub event_sender: broadcast::Sender<TransportEvent>,
    pub options: WebsocketTransportOptions,
    pub writable: bool,
    websocket_sender: SplitSink<WebSocket, Message>,
}

impl WebsocketTransport<AdapterWsHandle> for WarpWebsocketTransport {
    fn new(
        options: WebsocketTransportOptions,
        event_sender: broadcast::Sender<TransportEvent>,
        socket_handle: AdapterWsHandle,
    ) -> Self {
        let (websocket_sender, websocket_receiver) = socket_handle;
        subscribe_to_websocket_messages(websocket_receiver, event_sender.clone());
        WarpWebsocketTransport {
            event_sender,
            writable: true,
            options,
            websocket_sender,
        }
    }
}

#[async_trait]
impl TransportBase<AdapterResponse> for WarpWebsocketTransport {
    async fn close(&mut self) {
        unimplemented!();
    }

    fn discard(&self) {
        unimplemented!();
    }

    async fn send(&mut self, packets: Vec<Packet>) {
        let sender = &mut self.websocket_sender;

        if self.options.supports_binary {
            let messages = packets
                .into_iter()
                .map(|packet| Message::binary(binary_encoder::encode_packet(&packet)));
                // .collect();
            stream::iter(messages).map(Ok).forward(sender).await;
        } else {
            let messages = packets
                .into_iter()
                .map(|packet| Message::text(string_encoder::encode_packet(&packet)));
            // TODO: deal with errors and return it
            stream::iter(messages).map(Ok).forward(sender).await;
        }
    }

    fn is_writable(&self) -> bool {
        self.writable
    }
}

fn subscribe_to_websocket_messages(
    websocket_receiver: SplitStream<WebSocket>,
    event_sender: broadcast::Sender<TransportEvent>,
) {
    let mut websocket_receiver = websocket_receiver;
    let subscriber_task = async move {
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
                                if let Err(send_error) = event_sender.send(TransportEvent::Packet { packet }) {
                                    // TODO: trace this error
                                    println!("Failed to send TransportEvent::Packet [websocket] {:?}", send_error);
                                }
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
        if let Err(send_error) = event_sender.send(TransportEvent::Close) {
            // TODO: trace this error
            println!("Failed to send TransportEvent::Close {:?}", send_error);
        }
    };
    tokio::spawn(subscriber_task);
}
