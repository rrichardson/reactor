use std::net::{SocketAddr, lookup_host, SocketAddrV4, SocketAddrV6};
use std::io::{Error, ErrorKind};
use iobuf::AROIobuf;
use mio::tcp::{TcpStream, TcpListener};
use mio::util::{Slab};
use mio::{Token, EventLoop, Interest, PollOpt, ReadHint, Timeout, Handler};

use protocol::{Protocol, Message};
use connection::{Connection, OutBuf};

pub type TaggedBuf = (Token, AROIobuf);

pub type ProtocoRef = Rc<RefCell<Box<Protocol>>>;

type ConnRecord = (ProtocolRef, Box<Connection<Protocol>>);
type Listener = (ProtocolRef, TcpListener);

/// Configuration for the Reactor
/// queue_size: All queues, both inbound and outbound
pub struct ReactorConfig {
    pub out_queue_size: usize,
    pub max_connections: usize,
    pub timers_per_connection: usize,
    pub poll_timeout_ms: usize,
}

pub struct ReactorInner
{
    pub listeners: RefCell<Slab<Listener>>,
    pub conns: RefCell<Slab<ConnRecord>>,
    pub timeouts: RefCell<Slab<(u64, Option<Timeout>)>>,
    pub config: ReactorConfig,
    pub handler: H
}

impl ReactorInner
{

    pub fn new(handler: H, cfg: ReactorConfig) -> ReactorInner {
        let num_listeners = 255;
        let conn_slots = cfg.max_connections + num_listeners + 1;
        let timer_slots = (conn_slots * cfg.timers_per_connection);

        ReactorInner {
            listeners: RefCell::new(Slab::new_starting_at(Token(0), 255)),
            conns: RefCell::new(Slab::new_starting_at(Token(num_listeners + 1), conn_slots)),
            timeouts: RefCell::new(Slab::new_starting_at(Token(0), timer_slots)),
            handler: handler,
            config: cfg
        }
    }

    pub fn connect<'b>(&self,
                   addr: &'b str,
                   port: usize,
                   event_loop: &mut EventLoop<ReactorInner<P, H>>,
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
                      event_loop: &mut EventLoop<ReactorInner<P, H>>,
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

    pub fn accept(&self, event_loop: &mut EventLoop<ReactorInner<P, H>>,
                  token: Token, hint: ReadHint) -> Result<Token, Error> {

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

    pub fn on_read(&self, event_loop: &mut EventLoop<ReactorInner<P, H>>, token: Token, hint: ReadHint) {

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

impl<P, H> Handler for ReactorInner<P, H>
where P : Protocol, <P as Protocol>::Output : Send,
      H : Mailbox<P>
{
    type Timeout = u64;
    type Message = OutBuf;

    fn readable(&mut self, event_loop: &mut EventLoop<ReactorInner<P, H>>, token: Token, hint: ReadHint) {
        debug!("mio_processor::readable top, token: {:?}", token);
        if self.listeners.contains(token) {
            self.accept(event_loop, token, hint);
        } else {
            self.on_read(event_loop, token, hint);
        }
    }

    fn writable(&mut self, event_loop: &mut EventLoop<ReactorInner<P, H>>, token: Token) {
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


    fn notify(&mut self, event_loop: &mut EventLoop<ReactorInner<P, H>>, msg: TaggedBuf) {
        let tok = msg.0;
        if let Some(&mut (ref mut proto, ref mut c)) = self.conns.get_mut(tok) {
            if let Some(msg) = proto.notify(mid) {
                self.dispatch(msg):
            }
        }
    }

    fn timeout(&mut self, event_loop: &mut EventLoop<ReactorInner<P, H>>, timeout : u64) {
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

