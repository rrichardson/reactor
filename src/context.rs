
use mio::Interest;

enum EventType {
    Readable,
    Writable,
    Disconnect,
    Connect,
    Timeout(uint)
}

trait Context {
    type Socket : Evented

    fn get_socket(self) -> Self::Socket;

    fn on_event(&mut self, evt : EventType);

    fn get_interest(self) -> Interest;
}
