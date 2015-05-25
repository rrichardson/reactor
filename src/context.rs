
use mio::{Interest, Evented};
use iobuf::AROIobuf;
use reactor_ctrl::ReactorCtrl;


pub enum EventType {
    Readable,
    Writable,
    Disconnect,
    Connect,
    Notify(AROIobuf),
    Timeout(usize)
}



pub trait Context {
    type Socket : Evented + ?Sized;

    fn on_event(&mut self, &mut ReactorCtrl, EventType);

    fn get_evented(&self) -> &Evented; //&Self::Socket;

    fn get_interest(&self) -> Interest;
}
