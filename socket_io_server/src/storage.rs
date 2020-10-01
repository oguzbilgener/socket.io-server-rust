
pub trait Storage: 'static + Send + Sync + Sized {

    fn new() -> Self;
}