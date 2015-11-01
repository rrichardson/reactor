
use mio::{EventSet, Evented};
use tendril::{Tendril, Atomic};
use tendril::fmt::Bytes;

use reactor_ctrl::ReactorCtrl;

///The event types that will be handled by \Context::on_event
pub enum EventType {
    ///Socket has data waiting to be read
    Readable,
    ///Socket has space available in its buffer for writing
    Writable,
    ///Remote end of the socket has disconnected
    Disconnect,
    ///Notify queue has received a message addressed to this socket
    Notify(Tendril<Bytes, Atomic>),
    ///A timeout designated for this socket (via timeout_conn) has fired
    Timeout(usize)
}


/// An abstraction over a socket or some other poll-able
/// descriptor. Presently, anything that implements
/// [mio::Evented](http://rustdoc.s3-website-us-east-1.amazonaws.com/mio/master/mio/trait.Evented.html)
/// will do.
pub trait Context {
    ///The primary event handler for the socket abstraction
    fn on_event(&mut self, &mut ReactorCtrl, EventType);

    ///returns the socket so that it can be registered with the event-loop
    fn get_evented(&self) -> &Evented; //&Self::Socket;

    ///returns the current event interest for the loop to register with the poller
    fn get_interest(&self) -> EventSet;
}
