use std::mem;

use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::EventLoop;

use reactor_handler::ReactorHandler;

pub type TaggedBuf = (Token, AROIobuf);

type ConnHandler = FnMut(TcpStream) -> Box<Context<Socket=Evented>>;
type ListenRec = (TcpListener, Box<ConnHandler>);
type PendingRec = (TcpStream, Box<ConnHandler>);

/// Configuration for the Reactor
/// queue_size: All queues, both inbound and outbound
pub struct ReactorConfig {
    pub out_queue_size: usize,
    pub max_connections: usize,
    pub timers_per_connection: usize,
    pub poll_timeout_ms: usize,
}

struct ReactorState {
    listeners: Slab<Listener>,
    conns: Slab<ConnRec>,
    timeouts: Slab<(u64, Option<Timeout>)>,
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
                   proto: ProtocolRef) -> Result<Token, Error>
    {
        let saddr = try!(lookup_host(addr).and_then(|lh| lh.nth(0)
                            .ok_or(Error::last_os_error()))
                        .and_then(move |sa| { match sa {
                            Ok(SocketAddr::V4(sa4)) => Ok(SocketAddr::V4(SocketAddrV4::new(*sa4.ip(), port as u16))),
                            Ok(SocketAddr::V6(sa6)) => Ok(SocketAddr::V6(SocketAddrV6::new(*sa6.ip(), port as u16, 0, 0))),
                            Err(_) => return Err(Error::new(ErrorKind::Other, "Failed to parse Supplied socket address"))
                        }}));
        let sock = try!(TcpStream::connect(&saddr));
        let tok = try!(self.conns.insert((proto.clone(), Box::new(Connection::new(sock))))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        try!(event_loop.register_opt(&self.conns.get_mut(tok).unwrap().1.sock,
                                     tok, Interest::readable(), PollOpt::edge()));
        Ok(tok)
    }

    pub fn listen<'b>(&self,
                      addr: &'b str,
                      port: usize,
                      proto: ProtocolRef) -> Result<Token, Error>
    {
        let saddr : SocketAddr = try!(addr.parse()
                .map_err(|_| Error::new(ErrorKind::Other, "Failed to parse address")));
        let server = try!(TcpListener::bind(&saddr));
        let tok = try!(self.listeners.insert((proto, server))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
        let &mut (ref proto, ref mut l) = self.conns.get_mut(tok).unwrap();
        try!(event_loop.register_opt(l, tok, Interest::readable(), PollOpt::edge()));
        Ok(tok)
    }

    pub fn accept(&self, token: Token, hint: ReadHint) -> Result<Token, Error> {

        let &mut (ref proto, ref mut accpt) = *self.listeners.get_mut(token).unwrap();

        if let Some(sock) = try!(accpt.accept()) {
            let peeraddr = try!(conn.sock.peer_addr());
            if !proto.borrow_mut().pre_accept(peeraddr) {
                return Error::new(ErrorKind::Other, format!("Connection from {} rejected", peeraddr));
            }
            let tok = try!(self.conns.insert((proto.clone(), Box::new(Connection::new(sock))))
                .map_err(|_|Error::new(ErrorKind::Other, "Failed to insert into slab")));
            let &mut (ref proto, ref mut conn) = self.conns.get_mut(tok).unwrap();
            if let Some(int) = proto.borrow_mut().on_accept(conn,  peeraddr) {
                try!(event_loop.register_opt(conn.sock,
                    tok, int | Interest::hup(), PollOpt::edge()));
            }
            try!(event_loop.reregister(accpt, token, Interest::readable(), PollOpt::edge()));
            return Ok(tok);
        }
        else {
            Err(Error::last_os_error())
        }
    }

    pub fn on_read(&self, event_loop: &mut EventLoop<ReactorHandler>, token: Token, hint: ReadHint) {

        let mut close = false;
        let &mut (ref proto, ref mut conn) = self.conns.get_mut(token).unwrap();
        match conn.state {
            InProgress => {
                conn.state = ConnectionState::Ready;
                if let Some(int) = proto.borrow_mut().on_connect(conn) {
                    try!(event_loop.register_opt(conn.sock,
                        tok, int | Interest::hup(), PollOpt::edge()));
                }
            },
            Ready => {
                if let Some(int) = tup.0.on_data(conn) {
                    try!(event_loop.reregister(accpt, token, int | Interest::hup(), PollOpt::edge()));
                }
            }
        }
        if hint.contains(ReadHint::hup()) {
            proto.on_disconnect(conn);
            self.conns.remove(token);
        }
    }

}
