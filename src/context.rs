
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

    fn get_evented<'a>(&self) -> &'a Self::Socket;

    fn get_writer<'a>(&self) -> &'a TryWrite;

    fn on_event(&mut self, evt : EventType);

    fn get_interest(self) -> Interest;
}
