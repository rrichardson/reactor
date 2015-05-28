use std::io::{Error, ErrorKind};

use mio::{Token,
          EventLoop,
          Interest,
          PollOpt,
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
    pub state: Option<ReactorState>
}

impl ReactorHandler {
    fn on_read(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {

        let close = hint.is_hup() || hint.is_error();
        let mut state = self.state.as_mut().unwrap();

        match state.conns.replace(token, ConnRec::None) {
            Some(ConnRec::Pending(sock, mut handler)) => {
                if close {
                    //TODO Add exact error message from sockopt
                    handler(ConnResult::Failed(Error::new(ErrorKind::ConnectionRefused, "")));
                    return;
                }
                let peeraddr = sock.peer_addr().unwrap();
                if let Some(ctx) = handler(ConnResult::Connected(sock, token, peeraddr)) {
                    event_loop.register_opt(ctx.get_evented(),
                        token, ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                    state.conns.replace(token, ConnRec::Connected(ctx));
                }
                else {
                    debug!("Outbound connection to {} rejected", peeraddr);
                }
            },
            Some(ConnRec::Connected(mut ctx)) => {
                ctx.on_event(&mut ReactorCtrl::new(&mut *state, event_loop),
                             EventType::Readable);
                if close {
                    ctx.on_event(&mut ReactorCtrl::new(&mut *state, event_loop),
                                 EventType::Disconnect);
                }else {
                    event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                }
                state.conns.replace(token, ConnRec::Connected(ctx));
            },
            _ => {panic!("Got a readable event for an unregistered token");}
        }
    }

    fn accept(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token) {

        let mut state = self.state.as_mut().unwrap();
        if let Some((accpt, mut handler)) = state.listeners.replace(token, None).unwrap() {
            if let Some(sock) = accpt.accept().unwrap() {
                event_loop.reregister(&accpt, token, Interest::readable(), PollOpt::edge()).unwrap();

                let peeraddr = sock.peer_addr().unwrap();
                let newtok = state.conns.insert(ConnRec::None)
                    .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")).unwrap();
                if let Some(ctx) = handler(ConnResult::Connected(sock, newtok, peeraddr)) {
                    state.conns[newtok] = ConnRec::Connected(ctx);
                }
                else {
                    debug!("Connection from {} rejected", peeraddr);
                }
            }
            else {
                error!("{}", Error::last_os_error());
            }
            state.listeners.replace(token, Some((accpt, handler)));
        }
    }
}

impl Handler for ReactorHandler
{
    type Timeout = usize;
    type Message = TaggedBuf;

    fn readable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {
        debug!("mio_processor::readable top, token: {:?}", token);
        if self.state.as_ref().unwrap().listeners.contains(token) {
            self.accept(event_loop, token);
        } else {
            self.on_read(event_loop, token, hint);
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<ReactorHandler>, token: Token) {
        debug!("mio_processor::writable, token: {:?}", token);
        let mut state = self.state.as_mut().unwrap();
        match state.conns.replace(token, ConnRec::None) {
            Some(ConnRec::Connected(mut ctx)) => {
                ctx.on_event(&mut ReactorCtrl::new(state, event_loop),
                    EventType::Readable);
                event_loop.reregister(ctx.get_evented(), token,
                    ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                state.conns.replace(token, ConnRec::Connected(ctx));
            },
            Some(ConnRec::Pending(_, _)) => { panic!("Got a writable event for a pending socket connection"); },
            _ => { panic!("Got a writable event for a non-present context") }
        }
    }


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorHandler>, msg: TaggedBuf) {
        let token = msg.0;
        let mut state = self.state.as_mut().unwrap();
        match state.conns.replace(token, ConnRec::None) {
            Some(ConnRec::Connected(mut ctx)) => {
                ctx.on_event(&mut ReactorCtrl::new(state, event_loop),
                    EventType::Notify(msg.1));
                event_loop.reregister(ctx.get_evented(), token,
                    ctx.get_interest() | Interest::hup(), PollOpt::edge()).unwrap();
                state.conns.replace(token, ConnRec::Connected(ctx));
            },
            Some(ConnRec::Pending(_,_)) => { panic!("Got a notify event for a pending socket connection"); },
            _ => { panic!("Got a notify event for a non-present context") }
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorHandler>, timeout : usize) {

        let tok = Token(timeout as usize);
        let mut state = self.state.as_mut().unwrap();
        let mut rec = state.timeouts.remove(tok);

        match rec {
            Some((Some(tok), None)) => {
                if let Some(mut conn) = state.conns.replace(tok, ConnRec::None) {
                    match conn {
                        ConnRec::Connected(ref mut ctx) => {
                            ctx.on_event(&mut ReactorCtrl::new(&mut *state, event_loop),
                                EventType::Timeout(timeout))
                        },
                        ConnRec::Pending(_,_) => {
                            panic!("Got a timeout event for a pending socket connection");
                        },
                        ConnRec::None => {
                            panic!("Got a timeout event for a non-present context")
                        }
                    }
                    state.conns.replace(tok, conn);
                }
            },
            Some((None, Some(ref mut handler))) => {
                handler(Token(timeout))
            },
            _ => {panic!("We shouldn't be here")}
        }
    }

}

