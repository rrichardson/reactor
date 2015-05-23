use std::mem;

use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token, EventLoop};

use reactor_handler::ReactorHandler;
use context::{Context, EventType};

pub type TaggedBuf = (Token, AROIobuf);

type ConnHandler = FnMut(TcpStream, Token, SockAddr) -> Option<Box<Context<Socket=Evented>>>;
type TimeoutHandler = FnMut(Token);

type ListenRec = (TcpListener, Box<ConnHandler>);
type TimerRec = (Option<Token>, Option<Box<TimeoutHandler>>);

pub enum ConnRec {
    Connected(Box<Context<Socket=Evented>>),
    Pending(TcpStream, Box<ConnHandler>),
    None
}

/// Configuration for the Reactor
/// queue_size: All queues, both inbound and outbound
pub struct ReactorConfig {
    pub out_queue_size: usize,
    pub max_connections: usize,
    pub timers_per_connection: usize,
    pub poll_timeout_ms: usize,
}

struct ReactorState {
    listeners: Slab<ListenRec>,
    conns: Slab<ConnRec>,
    timeouts: Slab<(TimerRec)>,
    config: ReactorConfig,
}

impl ReactorState {

    pub fn new(cfg: ReactorConfig) -> ReactorHandler {
        let num_listeners = 255;
        let conn_slots = cfg.max_connections + num_listeners + 1;
        let timer_slots = (conn_slots * cfg.timers_per_connection);

        ReactorHandler {
            listeners: RefCell::new(Slab::new_starting_at(Token(0), 255)),
            conns: RefCell::new(Slab::new_starting_at(Token(num_listeners + 1), conn_slots)),
            timeouts: RefCell::new(Slab::new_starting_at(Token(0), timer_slots)),
            config: cfg,
        }
    }
}


struct ReactorCtrl<'a> {
    state: &'a mut ReactorState,
    event_loop: &'a EventLoop<ReactorHandler>
};

impl<'a> ReactorCtrl<'a> {

    pub fn new(st: &'a mut ReactorState,
        el : &'a EventLoop<ReactorHandler>) -> ReactorCtrl<'a>
    {
        ReactorCtrl {
            state: st,
            event_loop: el
        }
    }

    pub fn connect<'b>(&self,
                   addr: &'b str,
                   port: usize,
                   handler: Box<ConnHandler>) -> Result<Token, Error>
    {
        let saddr = try!(lookup_host(addr).and_then(|lh| lh.nth(0)
                            .ok_or(Error::last_os_error()))
                        .and_then(move |sa| { match sa {
                            Ok(SocketAddr::V4(sa4)) => Ok(SocketAddr::V4(SocketAddrV4::new(*sa4.ip(), port as u16))),
                            Ok(SocketAddr::V6(sa6)) => Ok(SocketAddr::V6(SocketAddrV6::new(*sa6.ip(), port as u16, 0, 0))),
                            Err(_) => return Err(Error::new(ErrorKind::Other, "Failed to parse Supplied socket address"))
                        }}));
        let sock = try!(TcpStream::connect(&saddr));
        let tok = try!(self.state.conns.insert(ConnRec::Pending(sock, handler))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        try!(self.event_loop.register_opt(&self.conns.get_mut(tok).unwrap().1.sock, tok, Interest::readable(), PollOpt::edge()));
        Ok(tok)
    }

    pub fn listen<'b>(&self,
                      addr: &'b str,
                      port: usize,
                      handler: Box<ConnHandler>) -> Result<Token, Error>
    {
        let saddr : SocketAddr = try!(addr.parse()
                .map_err(|_| Error::new(ErrorKind::Other, "Failed to parse address")));
        let server = try!(TcpListener::bind(&saddr));
        let tok = try!(self.state.listeners.insert((server,handler))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        let &mut (ref proto, ref mut l) = self.conns.get_mut(tok).unwrap();
        try!(event_loop.register_opt(l, tok, Interest::readable(), PollOpt::edge()));
        Ok(tok)
    }

    /// fetch the event_loop channel for notifying the event_loop of new outbound data
    pub fn channel(&self) -> Sender<TaggedBuf> {
        self.event_loop.channel()
    }

    /// Set a timeout to be executed by the event loop after duration
    /// Minimum expected resolution is the tick duration of the event loop
    /// poller, but it could be shorted depending on how many events are
    /// occurring
    pub fn timeout(&mut self, duration: u64, handler: Box<TimeoutHandler>) -> TimerResult<(Timeout, Token)> {
        let tok = self.state.timeouts.insert((None, Some(handler))).map_err(|_| format!("failed")).unwrap();
        let handle = try!(self.event_loop.timeout_ms(tok.0, duration));
        Ok(handle, tok)
    }

    pub fn timeout_conn(&mut self, duration: u64, ctxtok: Token) -> TimerResult<Timeout> {
        let tok = self.state.timeouts.insert((Some(ctxtok),None)).map_err(|_| format!("failed")).unwrap();
        let handle = try!(self.event_loop.timeout_ms(tok.0, duration));
        Ok(handle, tok)
    }

    pub fn register(&mut self, ctx : Box<Context<Socket=Evented>>, interest: Interest) -> Result<Token> {

        let token = try!(self.state.conns.insert(ConnRec::Connected(ctx))
            .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));

        try!(self.event_loop.register_opt(ctx.get_evented(), token, ctx.get_interest(), PollOpt::edge()));
        Ok(token)
    }

    pub fn deregister(&mut self, token: Token) -> Result<Box<Context<Socket=Evented>>> {
        if let Some(conn) = self.state.conns.remove(token) {
            match conn {
                ConnRec::Connected(ctx) => {
                    self.event_loop.deregister(ctx.get_evented());
                    Ok(ctx)
                }
                ConnRec::Pending((sock, _)) => {
                    self.event_loop.deregister(sock);
                    Error::new(ErrorKind::Other, "Connection for token was pending, no context to return");
                }
                _ => {
                    Error::new(ErrorKind::Other, "No context for Token");
                }
            }
        }
        else {
            Error::new(ErrorKind::Other, "No context for Token");
        }
    }

    /// calculates the 11th digit of pi
    pub fn shutdown(&mut self) {
        self.event_loop.shutdown();
    }
}
