use std::net::{SocketAddr,
               lookup_host,
               SocketAddrV4,
               ToSocketAddrs,
               SocketAddrV6};
use std::io::{Error, ErrorKind, Result};

use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token,
          Evented,
          EventLoop,
          Interest,
          PollOpt,
          Timeout,
          Sender,
          TimerResult};

use iobuf::AROIobuf;

use reactor_handler::ReactorHandler;
use context::{Context};

pub type TaggedBuf = (Token, AROIobuf);

pub enum ConnResult {
    Connected(TcpStream, Token, SocketAddr),
    Failed(Error)
}

pub type ConnHandler<'a> = FnMut(ConnResult, &mut ReactorCtrl) -> Option<Box<Context>> + 'a;
pub type TimeoutHandler<'a> = FnMut(Token, &mut ReactorCtrl) + 'a;

pub type ListenRec<'a> = Option<(TcpListener, Box<ConnHandler<'a>>)>;
pub type TimerRec<'a> = (Option<Token>, Option<Box<TimeoutHandler<'a>>>);

pub enum ConnRec<'a> {
    Connected(Box<Context>),
    Pending(TcpStream, Box<ConnHandler<'a>>),
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

pub struct ReactorState<'a> {
    pub listeners: Slab<ListenRec<'a>>,
    pub conns: Slab<ConnRec<'a>>,
    pub timeouts: Slab<(TimerRec<'a>)>,
    pub config: ReactorConfig,
}

impl<'a> ReactorState<'a> {

    pub fn new(cfg: ReactorConfig) -> ReactorState<'a> {
        let num_listeners = 255;
        let conn_slots = cfg.max_connections + num_listeners + 1;
        let timer_slots = conn_slots * cfg.timers_per_connection;

        ReactorState {
            listeners: Slab::new_starting_at(Token(0), 255),
            conns: Slab::new_starting_at(Token(num_listeners + 1), conn_slots),
            timeouts: Slab::new_starting_at(Token(0), timer_slots),
            config: cfg,
        }
    }
}

/// ReactorCtrl is the event-loop control interface which is passed to every
/// handler, both the listen/connect handlers as well as the mailbox for
/// every Context that is managed by Reactor
pub struct ReactorCtrl<'a, 'b : 'a> {
    state: &'a mut ReactorState<'b>,
    event_loop: &'a mut EventLoop<ReactorHandler<'b>>
}

impl<'a, 'b : 'a> ReactorCtrl<'a, 'b> {

    #[doc(hidden)]
    pub fn new(st: &'a mut ReactorState<'b>,
        el : &'a mut EventLoop<ReactorHandler<'b>>) -> ReactorCtrl<'a, 'b>
    {
        ReactorCtrl {
            state: st,
            event_loop: el
        }
    }

    /// Attempt a connection to the remote host specified at the remote hostname or ip address
    /// and the port.  This is a connection on a non-blocking socket, so the connect call will
    /// return immediately.  It requires a handler, to which it will supply a `ConnResult` which
    /// will indicate success or failure. On success, it will supply a socket, a token, and a
    /// remote IP addr. It then expects an Option<Box<`Context`>> so that it can manage its events
    pub fn connect<'c>(&mut self,
                   hostname: &'c str,
                   port: usize,
                   handler: Box<ConnHandler<'b>>) -> Result<Token>
    {
        let saddr = try!(lookup_host(hostname).and_then(|ref mut lh| lh.nth(0)
                            .ok_or(Error::last_os_error()))
                        .and_then(move |sa| { match sa {
                            Ok(SocketAddr::V4(sa4)) =>
                                Ok(SocketAddr::V4(SocketAddrV4::new(*sa4.ip(), port as u16))),
                            Ok(SocketAddr::V6(sa6)) =>
                                Ok(SocketAddr::V6(SocketAddrV6::new(*sa6.ip(), port as u16, 0, 0))),
                            Err(_) => return Err(Error::new(ErrorKind::Other,
                                "Failed to parse Supplied socket address"))
                        }}));
        let sock = try!(TcpStream::connect(&saddr));
        let tok = try!(self.state.conns.insert(ConnRec::None)
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        try!(self.event_loop.register_opt(&sock, tok, Interest::writable(), PollOpt::edge()));
        self.state.conns[tok] = ConnRec::Pending(sock, handler);
        Ok(tok)
    }

    /// Listen on the supplied IP address:port for incoming TCP connections.  This returns
    /// immediately and expects a handler to which it will supply `ConnResult` and expect
    /// Option<Box<`Context`>> as a result
    pub fn listen<A : ToSocketAddrs>(&mut self,
                      addr: A,
                      handler: Box<ConnHandler<'b>>) -> Result<Token>
    {
        let saddr : SocketAddr = try!(addr.to_socket_addrs().and_then(|ref mut a| a.nth(0).ok_or(Error::last_os_error())));
        let server = try!(TcpListener::bind(&saddr));
        let tok = try!(self.state.listeners.insert(Some((server,handler)))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        if let &mut Some((ref server, _)) = self.state.listeners.get_mut(tok).unwrap() {
            try!(self.event_loop.register_opt(server, tok, Interest::readable(), PollOpt::edge()));
        }
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
    pub fn timeout(&mut self, duration: u64, handler: Box<TimeoutHandler<'b>>) -> TimerResult<(Timeout, Token)> {
        let tok = self.state.timeouts.insert((None, Some(handler))).map_err(|_| format!("failed")).unwrap();
        let handle = try!(self.event_loop.timeout_ms(tok.0, duration));
        Ok((handle, tok))
    }

    /// Set a timeout to be executed by the handler of a Context for a given token.
    /// This is useful for protocols which have timeouts or timed ping/pongs such as IRC.
    pub fn timeout_conn(&mut self, duration: u64, ctxtok: Token) -> TimerResult<(Timeout, Token)> {
        let tok = self.state.timeouts.insert((Some(ctxtok),None)).map_err(|_| format!("failed")).unwrap();
        let handle = try!(self.event_loop.timeout_ms(tok.0, duration));
        Ok((handle, tok))
    }

    /// Supply a context to the event_loop for monitoring and get back a token
    pub fn register<C>(&mut self, ctx : C) -> Result<Token>
    where C : Context + 'static
    {
        let foo : Box<Context> = Box::new(ctx);
        let token = try!(self.state.conns.insert(ConnRec::None)
            .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));

        try!(self.event_loop.register_opt(foo.get_evented() as &Evented, token, foo.get_interest(), PollOpt::edge()));

        self.state.conns[token] = ConnRec::Connected(foo);
        Ok(token)
    }

    /// deregister a context for a given token and receive back the context
    /// NOTE : You cannot deregister the context for a token while running in the
    /// handler of that context. It must be called for a different context
    pub fn deregister(&mut self, token: Token) -> Result<Box<Context>>
    {
        if let Some(conn) = self.state.conns.remove(token) {
            match conn {
                ConnRec::Connected(ctx) => {
                    self.event_loop.deregister(ctx.get_evented()).unwrap();
                    Ok(ctx)
                }
                ConnRec::Pending(sock, _) => {
                    self.event_loop.deregister(&sock).unwrap();
                    Err(Error::new(ErrorKind::Other, "Connection for token was pending, no context to return"))
                }
                _ => {
                    Err(Error::new(ErrorKind::Other, "No context for Token"))
                }
            }
        }
        else {
            Err(Error::new(ErrorKind::Other, "No context for Token"))
        }
    }

    /// calculates the 11th digit of pi
    pub fn shutdown(&mut self) {
        self.event_loop.shutdown();
    }
}
