use std::net::{SocketAddr, lookup_host, SocketAddrV4, SocketAddrV6};
use std::io::{Error, ErrorKind};
use iobuf::AROIobuf;
use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token, EventLoop, Interest, PollOpt, ReadHint, Timeout, Handler};

use context::Context;
use reactor_ctrl::{ReactorCtrl, ConnRec, ReactorState};

pub struct ReactorHandler
{
    state: Rc<RefCell<ReactorState>>
}

impl ReactorHandler {
    fn on_read(&self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {

        let mut close = false;
        let &mut (ref mut conn) = self.state.borrow_mut().conns.get_mut(token).unwrap();
        let close = hint.hup() || hint.error();
        match conn {
            ConnRec::Pending(sock, handler) => {
                if close {
                    return Error::new(ErrorKind::Other, format!("Outbound connection to {} failed", peeraddr));
                }
                let peeraddr = sock.peer_addr().unwrap();
                if let some(ctx) = handler(sock, token, peeraddr) {
                    try!(self.event_loop.register_opt(sock,
                        token, cxt.get_interest() | Interest::hup(), PollOpt::edge()));
                }
                else {
                    Error::new(ErrorKind::Other, format!("Outbound connection to {} rejected", peeraddr));
                }
            },
            ConnRec::Connected(ctx) => {
                ctx.on_event(ReactorCtrl::new(self.state.borrow_mut(), event_loop), EventType::Readable);
                if close {
                    ctx.on_event(ReactorCtrl::new(self.state.borrow_mut(), event_loop), EventType::Disconnect);
                }else {
                    try!(event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | Interest::hup(), PollOpt::edge()));
                }
            },
            None => {panic!("Got a readable event for a non-present Context");}
        }
    }

    fn accept(&self, token: Token) -> Result<Token, Error> {

        let &mut (ref mut accpt, ref mut handler) = self.listeners.get_mut(token).unwrap();

        if let Some(sock) = try!(accpt.accept()) {
            try!(event_loop.reregister(accpt, token, Interest::readable(), PollOpt::edge()));

            let peeraddr = try!(sock.peer_addr());
            let newtok = try!(self.state.borrow_mut().conns.insert(ConnRec::None)
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
            if let Some(ctx) = handler(sock, newtok, peeraddr) {
                self.state.borrow_mut().conns.get_mut(newtok).unwrap() = ConnRec::Connected(ctx)
                Ok(newtok);
            }
            else {
                Error::new(ErrorKind::Other, format!("Connection from {} rejected", peeraddr));
            }
        }
        else {
            Err(Error::last_os_error())
        }
    }
}

impl Handler for ReactorHandler
{
    type Timeout = usize;
    type Message = TaggedBuf;

    fn readable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {
        debug!("mio_processor::readable top, token: {:?}", token);
        if self.state.borrow().listeners.contains(token) {
            self.accept(event_loop, token);
        } else {
            self.on_read(event_loop, token, hint);
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token) {
        debug!("mio_processor::writable, token: {:?}", token);

        if let Some(conn) = self.state.borrow_mut().conns.get_mut(token) {
            match conn {
                ConnRec::Connected(ctx) {
                    ctx.on_event(ReactorCtrl::new(self.state.borrow_mut(), event_loop), EventType::Readable);
                    try!(event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | Interest::hup(), PollOpt::edge()));
                },
                ConnRec::Pending(_) { panic!("Got a writable event for a pending socket connection"); },
                ConnRec::None { panic!("Got a writable event for a non-present context") }
            }
        }
    }


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorHandler>, msg: TaggedBuf) {
        let token = msg.0;
        if let Some(conn) = self.state.borrow_mut().conns.get_mut(token) {
            match conn {
                ConnRec::Connected(ctx) {
                    ctx.on_event(ReactorCtrl::new(self.state.borrow_mut(), event_loop), EventType::Notify(msg.1));
                    try!(event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | Interest::hup(), PollOpt::edge()));
                },
                ConnRec::Pending(_) { panic!("Got a notify event for a pending socket connection"); },
                ConnRec::None { panic!("Got a notify event for a non-present context") }
            }
        }
        else {
            panic!("Got a notify event for an un-managed socket");
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorHandler>, timeout : usize) {

        let &mut rec = self.state.borrow_mut().timeouts.get_mut(Token(timeout as usize)).unwrap();
        match rec {
            (ref Some(tok), None) => {
                if let &mut (ref mut conn) = self.conns.get_mut(tok) {
                    ConnRec::Connected(ctx) {
                        ctx.on_event(ReactorCtrl::new(self.state.borrow_mut(), event_loop), EventType::Timeout(timeout));
                    },
                    ConnRec::Pending(_) { panic!("Got a timeout event for a pending socket connection"); },
                    ConnRec::None { panic!("Got a timeout event for a non-present context") }
                }
            },
            (None, ref mut handler) => {
                handler(Token(timeout))
            },
            _ => {}
        }
        else {
            panic!("No record found for timeout id");
        }
    }

}

