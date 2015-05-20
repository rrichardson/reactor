use std::net::{SocketAddr, lookup_host, SocketAddrV4, SocketAddrV6};
use std::io::{Error, ErrorKind};
use iobuf::AROIobuf;
use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token, EventLoop, Interest, PollOpt, ReadHint, Timeout, Handler};

use context::Context;
use reactor_ctrl::*;

pub struct ReactorHandler
{
    state: Option<ReactorState>
}

impl Handler for ReactorHandler
{
    type Timeout = u64;
    type Message = TaggedBuf;

    fn readable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {
        debug!("mio_processor::readable top, token: {:?}", token);
        if self.listeners.contains(token) {
            self.ctrl.borrow_mute().accept(event_loop, token, hint);
        } else {
            self.ctrl.borrow_mute().on_read(event_loop, token, hint);
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token) {
        debug!("mio_processor::writable, token: {:?}", token);

        if let Some(&mut (ref mut proto, ref mut c)) = self.conns.get_mut(token) {

            let mut writable = true;
            while writable && c.outbuf.len() > 0 {
                let (result, sz, mid) = {
                    let (buf, mid) = c.front_mut().unwrap(); //shouldn't panic because of len() check
                    let sz = buf.len();
                    (self.sock.write(&mut OutBuf(*buf.0)), sz as usize, mid)
                };
                match result {
                    Ok(Some(n)) =>
                    {
                        debug!("Wrote {:?} out of {:?} bytes to socket", n, sz);
                        if n == sz {
                            self.outbuf.pop_front(); // we have written the contents of this buffer so lets get rid of it
                            if let Some(msg) = proto.on_write(mid) {
                                self.dispatch(msg):
                            }
                        }
                    },
                    Ok(None) => { // this is also very unlikely, we got a writable message, but failed
                        // to write anything at all.
                        debug!("Got Writable event for socket, but failed to write any bytes");
                        writable = false;
                    },
                    Err(e) => { error!("error writing to socket: {:?}", e); writable = false }
                }
            }

            if self.outbuf.len() > 0 {
                c.interest.insert(Interest::writable());
                event_loop.reregister(&c.sock, token, c.interest, PollOpt::edge()).unwrap();
            }
        }
    }


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorHandler>, msg: TaggedBuf) {
        let tok = msg.0;
        if let Some(&mut (ref mut proto, ref mut c)) = self.conns.get_mut(tok) {
            if let Some(msg) = proto.notify(mid) {
                self.dispatch(msg):
            }
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorHandler>, timeout : u64) {
        let &mut (ref cid, ref handle) = self.timeouts.get_mut(Token(timeout as usize)).unwrap();
        let &mut (ref proto, ref mut conn) = self.conns.get_mut(Token(*cid as usize)).unwrap();
        let mut delay : Option<(u64, u64)> = None;
        let mut clear = false;
        match proto.borrow_mut().on_timer(Token(timeout), conn, ctrl) {
            Replace((dly, id)) => delay = { delay = Some(dly, id); clear = true },
            Restart => { }
            RestartAndAdd((dly, id)) => delay = Some(dly, id),
            Clear => clear = true
        }
        if clear {
            event_loop.clear_timeout(handle.unwrap());
        }
        if let Some((dly, id)) = delay {
            event_loop.timeout_ms(
        }
    }
}

