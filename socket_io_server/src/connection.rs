use crate::namespace::{Namespace, NamespacePriv, SimpleNamespace};
use crate::socket::{Handshake, Socket};
use crate::storage::Storage;
use bytes::Bytes;
use core::fmt::Debug;
use socket_io_parser::decoder::{DecodeError, Decoder};
use socket_io_parser::Packet;
use std::collections::HashMap;

use std::iter::Iterator;
use std::sync::{Arc, Mutex};

struct State<S, D>
where
    S: 'static + Storage,
    D: 'static + Decoder,
{
    /// All the sockets owned by a connection
    pub(crate) sockets: HashMap<String, Socket>,
    /// A Mapping of namespace_name => (socket_id, Namespace)
    pub(crate) namespaces: HashMap<String, (String, SimpleNamespace<S>)>,
    pub(crate) decoder: Option<D>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AddToNamespaceSuccess {
    Added { socket_id: String },
    AlreadyExisting { socket_id: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DecodeBinarySuccess {
    Done { packet: Packet },
    InProgress,
}

pub struct Connection<S, D>
where
    S: 'static + Storage,
    D: 'static + Decoder,
{
    pub(crate) id: String,
    state: Mutex<State<S, D>>,
}

impl<S, D> Connection<S, D>
where
    S: 'static + Storage,
    D: 'static + Decoder,
{
    /// Create a new connection record and the first `Socket` client for the
    /// given namespace. This socket joins the given namespace.
    pub(crate) fn initialize(
        engine_connection_id: &str,
        handshake: Handshake,
        namespace: Arc<SimpleNamespace<S>>,
    ) -> (Self, String) {
        let mut connection = Self {
            id: engine_connection_id.to_owned(),
            state: Mutex::new(State {
                sockets: HashMap::new(),
                namespaces: HashMap::new(),
                decoder: None,
            }),
        };
        let socket_id = Self::create_socket(
            &mut connection.state.get_mut().unwrap(),
            handshake,
            namespace,
        );
        (connection, socket_id)
    }

    fn create_socket(
        state: &mut State<S, D>,
        handshake: Handshake,
        namespace: Arc<SimpleNamespace<S>>,
    ) -> String {
        let socket = Socket::new(handshake, namespace.get_name());
        let socket_id = socket.id.clone();
        state.sockets.insert(socket.id.clone(), socket);
        namespace.add_socket(&socket_id);
        socket_id
    }

    pub(crate) fn add_to_namespace(
        &self,
        handshake: Handshake,
        namespace: Arc<SimpleNamespace<S>>,
    ) -> AddToNamespaceSuccess {
        let mut state = self.state.lock().unwrap();
        if state.namespaces.contains_key(namespace.get_name()) {
            AddToNamespaceSuccess::AlreadyExisting {
                socket_id: state
                    .namespaces
                    .get(namespace.get_name())
                    .map(|record| record.0.clone())
                    .expect("Unexpected internal state error"),
            }
        } else {
            AddToNamespaceSuccess::Added {
                socket_id: Self::create_socket(&mut state, handshake, namespace),
            }
        }
    }

    pub(crate) fn iter_socket_ids_mut<'a>(
        &'a mut self,
    ) -> impl std::iter::Iterator<Item = &'a String> + 'a {
        self.state.get_mut().unwrap().sockets.keys()
    }

    pub(crate) fn decode_binary_data(
        &self,
        buffer: Bytes,
    ) -> Result<DecodeBinarySuccess, DecodeError<D>> {
        let mut state = self.state.lock().unwrap();
        if let Some(mut decoder) = state.decoder.take() {
            let done = decoder
                .decode_binary_data(buffer)
                .map_err(|err| DecodeError::DecodeFailed(err))?;
            if done {
                let packet = decoder.collect_packet().unwrap();
                Ok(DecodeBinarySuccess::Done { packet })
            } else {
                state.decoder = Some(decoder);
                Ok(DecodeBinarySuccess::InProgress)
            }
        } else {
            Err(DecodeError::InvalidDecoderState)
        }
    }

    pub(crate) fn set_decoder(&self, decoder: D) -> Result<(), DecodeError<D>> {
        let mut state = self.state.lock().unwrap();
        if state.decoder.is_none() {
            state.decoder = Some(decoder);
            Ok(())
        } else {
            Err(DecodeError::InvalidDecoderState)
        }
    }

    pub(crate) fn handle_packet(&self, packet: Packet) {
        // TODO: find the Socket instance and call a method which would trigger its external event listeners
        let state = self.state.lock().unwrap();
        if let Some((socket_id, _)) = state.namespaces.get(&packet.nsp) {
            if let Some(socket) = state.sockets.get(socket_id) {
                socket.handle_packet(packet);
            } else {
                // TODO: should return some kind of an error
            }
        } else {
            // TODO: should return some kind of an error
        }
    }
}

impl<S, D> Drop for State<S, D>
where
    S: 'static + Storage,
    D: 'static + Decoder,
{
    fn drop(&mut self) {
        self.namespaces
            .iter()
            .for_each(|(_name, (socket_id, namespace))| namespace.remove_socket(&socket_id));
    }
}
