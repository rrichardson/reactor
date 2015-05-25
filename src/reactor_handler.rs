use std::net::{SocketAddr,
               lookup_host, SocketAddrV4, SocketAddrV6};
use std::io::{Error, ErrorKind};
use std::rc::Rc;
use std::cell::RefCell;

use iobuf::AROIobuf;

use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token,
          EventLoop,
          Interest,
          PollOpt,
          Timeout,
          ReadHint,
          Handler};

use context::{Context, EventType};
use reactor_ctrl::{ReactorCtrl,
                   ConnRec,
                   ConnResult,
                   ReactorState,
                   TaggedBuf};

pub struct ReactorHandler
{
    pub state: Rc<RefCell<ReactorState>>
}

impl ReactorHandler {
    fn on_read(&self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {

        let mut state = self.state.borrow_mut();
        let conn = state.conns.get_mut(token).unwrap();
        let close = hint.is_hup() || hint.is_error();
        match conn {
            &mut ConnRec::Pending(ref sock, ref mut handler) => {
                if close {
                    //TODO Add exact error message from sockopt
                    handler(ConnResult::Failed(Error::new(ErrorKind::ConnectionRefused, "")));
                }
                let peeraddr = sock.peer_addr().unwrap();
                //FIXME what needs to be done here is we move the connection out of the slab
                //via remove, temporarily insert a None into the the slab, take the new token
                //token and pass that into the handler via Connected
                if let Some(ctx) = handler(ConnResult::Connected(*sock, token, peeraddr)) {
                    // TODO handle this error
                    event_loop.register_opt(ctx.get_evented(),
                        token, ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                    state.conns[token] = ctx; // TODO FIXME
                }
                else {
                    debug!("Outbound connection to {} rejected", peeraddr);
                }
            },
            &mut ConnRec::Connected(ref mut ctx) => {
                ctx.on_event(&mut ReactorCtrl::new(&mut *state, event_loop),
                             EventType::Readable);
                if close {
                    ctx.on_event(&mut ReactorCtrl::new(&mut *state, event_loop),
                                 EventType::Disconnect);
                }else {
                    event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                }
            },
            &mut ConnRec::None => {panic!("Got a readable event for a non-present Context");}
        }
    }

    fn accept(&self, event_loop: &mut EventLoop<ReactorHandler>, token: Token) {

        let &mut (ref mut accpt, ref mut handler) = self.state.borrow_mut().listeners.get_mut(token).unwrap();

        if let Some(sock) = accpt.accept().unwrap() {
            event_loop.reregister(accpt, token, Interest::readable(), PollOpt::edge()).unwrap();

            let peeraddr = sock.peer_addr().unwrap();
            let newtok = self.state.borrow_mut().conns.insert(ConnRec::None)
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")).unwrap();
            if let Some(ctx) = handler(ConnResult::Connected(sock, newtok, peeraddr)) {
                let c = self.state.borrow_mut().conns[newtok] = ConnRec::Connected(ctx);
            }
            else {
                debug!("Connection from {} rejected", peeraddr);
            }
        }
        else {
            error!("{}", Error::last_os_error());
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
                &mut ConnRec::Connected(ref mut ctx) => {
                    ctx.on_event(&mut ReactorCtrl::new(&mut (*self.state.borrow_mut()), event_loop),
                        EventType::Readable);
                    event_loop.reregister(ctx.get_evented(), token,
                        ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                },
                &mut ConnRec::Pending(_, _) => { panic!("Got a writable event for a pending socket connection"); },
                &mut ConnRec::None => { panic!("Got a writable event for a non-present context") }
            }
        }
    }


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorHandler>, msg: TaggedBuf) {
        let token = msg.0;
        if let Some(conn) = self.state.borrow_mut().conns.get_mut(token) {
            match conn {
                &mut ConnRec::Connected(ctx) => {
                    ctx.on_event(&mut ReactorCtrl::new(&mut (*self.state.borrow_mut()), event_loop),
                        EventType::Notify(msg.1));
                    event_loop.reregister(ctx.get_evented(), token,
                        ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                },
                &mut ConnRec::Pending(_,_) => { panic!("Got a notify event for a pending socket connection"); },
                &mut ConnRec::None => { panic!("Got a notify event for a non-present context") }
            }
        }
        else {
            panic!("Got a notify event for an un-managed socket");
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorHandler>, timeout : usize) {

        let &mut rec = self.state.borrow_mut().timeouts.get_mut(Token(timeout as usize)).unwrap();
        match rec {
            (Some(tok), None) => {
                if let Some(conn) = self.state.borrow_mut().conns.get_mut(tok) {
                    match conn {
                        &mut ConnRec::Connected(ctx) => {
                            ctx.on_event(&mut ReactorCtrl::new(&mut (*self.state.borrow_mut()), event_loop),
                                EventType::Timeout(timeout));
                        },
                        &mut ConnRec::Pending(_,_) => {
                            panic!("Got a timeout event for a pending socket connection");
                        },
                        &mut ConnRec::None => {
                            panic!("Got a timeout event for a non-present context")
                        }
                    }
                }
            },
            (None, Some(ref mut handler)) => {
                handler(Token(timeout))
            },
            _ => {}
        }
    }

}

