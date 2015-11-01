use std::io::{Error, ErrorKind};

use mio::{Token,
          EventLoop,
          EventSet,
          PollOpt,
          Handler};

use context::{Context, EventType};
use reactor_ctrl::{ReactorCtrl,
                   ConnRec,
                   ConnResult,
                   ReactorState,
                   TaggedBuf};

pub struct ReactorHandler<'a>
{
    pub state: Option<ReactorState<'a>>
}

impl<'a> ReactorHandler<'a> {
    fn on_read(&mut self, event_loop: &mut EventLoop<ReactorHandler<'a>>, token: Token, evts : EventSet) {

        let close = evts.is_hup() || evts.is_error();
        let mut state = self.state.as_mut().unwrap();

        match state.conns.replace(token, ConnRec::None) {
            Some(ConnRec::Pending(_, mut handler)) => {
                if close {
                    //TODO Add exact error message from sockopt
                    handler(ConnResult::Failed(Error::new(ErrorKind::ConnectionRefused, "")), &mut ReactorCtrl::new(&mut state, event_loop));
                    return;
                }
            },
            Some(ConnRec::Connected(mut ctx)) => {
                ctx.on_event(&mut ReactorCtrl::new(&mut state, event_loop),
                             EventType::Readable);
                if close {
                    ctx.on_event(&mut ReactorCtrl::new(&mut state, event_loop),
                                 EventType::Disconnect);
                }else {
                    event_loop.reregister(ctx.get_evented(), token, ctx.get_interest() | EventSet::hup(), PollOpt::edge()).unwrap();
                }
                state.conns.replace(token, ConnRec::Connected(ctx));
            },
            _ => {panic!("Got a readable event for an unregistered token");}
        }
    }

    fn accept(&mut self, event_loop: &mut EventLoop<ReactorHandler<'a>>, token: Token) {

        let mut state = self.state.as_mut().unwrap();
        if let Some((accpt, mut handler)) = state.listeners.replace(token, None).unwrap() {
            if let Some(sock) = accpt.accept().unwrap() {
                event_loop.reregister(&accpt, token, EventSet::readable(), PollOpt::edge()).unwrap();

                let peeraddr = sock.peer_addr().unwrap();
                let newtok = state.conns.insert(ConnRec::None)
                    .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")).unwrap();
                if let Some(ctx) = handler(ConnResult::Connected(sock, newtok, peeraddr),&mut ReactorCtrl::new(&mut state, event_loop)) {
                    event_loop.register_opt(ctx.get_evented(), newtok, ctx.get_interest() | EventSet::hup(), PollOpt::edge()).unwrap();
                    state.conns.replace(newtok, ConnRec::Connected(ctx));
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



impl<'a> Handler for ReactorHandler<'a>
{
    type Timeout = usize;
    type Message = TaggedBuf;

    /// Invoked when the socket represented by `token` is ready to be operated
    /// on. `events` indicates the specific operations that are
    /// ready to be performed.
    ///
    /// For example, when a TCP socket is ready to be read from, `events` will
    /// have `readable` set. When the socket is ready to be written to,
    /// `events` will have `writable` set.
    ///
    /// This function will only be invoked a single time per socket per event
    /// loop tick.
    fn ready(&mut self, event_loop: &mut EventLoop<Self>, token: Token, events: EventSet) {
        if events.is_readable() {
            debug!("mio_processor::readable top, token: {:?}", token);
            if self.state.as_ref().unwrap().listeners.contains(token) {
                self.accept(event_loop, token);
            } else {
                self.on_read(event_loop, token, events);
            }
        }
        else if events.is_writable() {
            debug!("mio_processor::writable, token: {:?}", token);
            let mut state = self.state.as_mut().unwrap();
            match state.conns.replace(token, ConnRec::None) {
                Some(ConnRec::Connected(mut ctx)) => {
                    ctx.on_event(&mut ReactorCtrl::new(state, event_loop),
                        EventType::Readable);
                    event_loop.reregister(ctx.get_evented(), token,
                        ctx.get_interest() | EventSet::hup(), PollOpt::edge()).unwrap();
                    state.conns.replace(token, ConnRec::Connected(ctx));
                },
                Some(ConnRec::Pending(sock, mut handler)) => {
                    let peeraddr = sock.peer_addr().unwrap();
                    if let Some(ctx) = handler(ConnResult::Connected(sock, token, peeraddr), &mut ReactorCtrl::new(&mut state, event_loop)) {
                        event_loop.reregister(ctx.get_evented(),
                            token, ctx.get_interest() | EventSet::hup(), PollOpt::edge()).unwrap();
                        state.conns.replace(token, ConnRec::Connected(ctx));
                    }
                    else {
                        debug!("Outbound connection to {} rejected", peeraddr);
                    }
                },
                _ => { panic!("Got a writable event for a non-present context") }
            }

        }
    }

    /// Invoked when `EventLoop` has been interrupted by a signal interrupt.
    fn interrupted(&mut self, _event_loop: &mut EventLoop<Self>) {
    }

    /// Invoked at the end of an event loop tick.
    fn tick(&mut self, _event_loop: &mut EventLoop<Self>) {
    }


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorHandler<'a>>, msg: TaggedBuf) {
        let token = msg.0;
        let mut state = self.state.as_mut().unwrap();
        match state.conns.replace(token, ConnRec::None) {
            Some(ConnRec::Connected(mut ctx)) => {
                ctx.on_event(&mut ReactorCtrl::new(state, event_loop),
                    EventType::Notify(msg.1));
                event_loop.reregister(ctx.get_evented(), token,
                    ctx.get_interest() | EventSet::hup(), PollOpt::edge()).unwrap();
                state.conns.replace(token, ConnRec::Connected(ctx));
            },
            Some(ConnRec::Pending(_,_)) => { panic!("Got a notify event for a pending socket connection"); },
            _ => { panic!("Got a notify event for a non-present context") }
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorHandler<'a>>, timeout : usize) {

        let tok = Token(timeout as usize);
        let mut state = self.state.as_mut().unwrap();
        let mut rec = state.timeouts.remove(tok);

        match rec {
            Some((Some(tok), None)) => {
                if let Some(mut conn) = state.conns.replace(tok, ConnRec::None) {
                    match conn {
                        ConnRec::Connected(ref mut ctx) => {
                            ctx.on_event(&mut ReactorCtrl::new(&mut state, event_loop),
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
                handler(Token(timeout),&mut ReactorCtrl::new(&mut state, event_loop))
            },
            _ => {panic!("We shouldn't be here")}
        }
    }

}

