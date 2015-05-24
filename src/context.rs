
use mio::{Interest, Evented};
use iobuf::AROIobuf;
use reactor_ctrl::ReactorCtrl;


enum EventType {
    Readable,
    Writable,
    Disconnect,
    Connect,
    Notify(AROIobuf),
    Timeout(usize)
}



trait Context {
    type Socket : Evented;

    fn on_event(&mut self, &mut ReactorCtrl, EventType);

    fn get_evented<'a>(&self) -> &'a Self::Socket;

    fn get_interest(self) -> Interest;
}
