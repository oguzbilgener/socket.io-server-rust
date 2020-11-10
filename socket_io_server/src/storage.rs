use std::collections::HashSet;

pub trait Storage: 'static + Send + Sync + Sized {

    fn new() -> Self;

    fn add_all(&self, socket_id: &str, room_names: HashSet<String>);

    fn del(&self, socket_id: &str, room_name: &str);
}