//!
//! Reactor is based around two notions, an event loop (Reactor) and a Context, which is the main
//! mailbox handler for a connection (or datagram receiver). A context is also the manager of the
//! socket abstraction itself. Reactor offers utilities to make working with these easier, but the
//! actual creation and interaction with the socket is out of the purview of reactor and is owned
//! entirely by the Context impl.
//!
//! # Example
//!
//! In this example, we create two Socket Contexts, one for the client connection, ClientConn, and one
//! for the accepting side of the connection, ServConn.
//! listen() takes a handler which we use to create the server side of the connection, when we
//! receive the connection, we start a timer for that connection, which we handle in the context's
//! mailbox. The timer event will keep reregistering itself unless the count has reached 5, at
//! which point it shuts down the event_loop and the program ends.
//!
//! Connect also takes a handler, which we use to construct the Client context when the connection
//! completes.
//!
//!```
//!
//! #![feature(core)]
//!
//! extern crate reactor;
//!
//! use std::slice::bytes::copy_memory;
//! use std::string::String;
//! use std::io::{Read, Write};
//! use reactor::{ReactorCtrl,
//!               Reactor,
//!               ConnResult,
//!               Evented,
//!               Interest,
//!               Context,
//!               EventType,
//!               Token};
//!
//! use reactor::tcp::{TcpStream};
//!
//!
//!struct ClientConn {
//!    interest: Interest,
//!    sock : TcpStream,
//!    token : Token
//!}
//!
//!impl Context for ClientConn {
//!
//!    //the event handler for the client connection, for this example,
//!    // we only care about readable events, we ignore the rest
//!    fn on_event(&mut self, ctrl : &mut ReactorCtrl, evt : EventType) {
//!        match evt {
//!            EventType::Readable => {
//!                let mut buf : [u8; 5] = [0; 5];
//!                let num = self.sock.read(&mut buf).unwrap();
//!                if num == 5 {
//!                    let msg = String::from_utf8_lossy(&buf).into_owned();
//!
//!                    if msg == "PING!" {
//!                        copy_memory("PONG!".as_bytes(), &mut buf);
//!                        self.sock.write(&buf).unwrap();
//!                    }
//!                }
//!            },
//!            _ => {}
//!        }
//!    }
//!
//!    fn get_evented(&self) -> &Evented {
//!        &self.sock as &Evented
//!    }
//!
//!    fn get_interest(&self) -> Interest {
//!        self.interest
//!    }
//!}
//!
//!struct ServConn {
//!    interest: Interest,
//!    sock : TcpStream,
//!    token : Token,
//!    count : u32,
//!}
//!
//!// here we indicate that we wish to handle only Timeout and Readable
//!// events, and ignore the rest
//!impl Context for ServConn {
//!    fn on_event(&mut self, ctrl : &mut ReactorCtrl, evt : EventType) {
//!        match evt {
//!            EventType::Timeout(_) => {
//!                if self.count < 5 {
//!                    let mut buf : [u8; 5] = [0; 5];
//!                    copy_memory("PING!".as_bytes(), &mut buf);
//!                    self.sock.write(&buf).unwrap();
//!                    self.count += 1;
//!                    ctrl.timeout_conn(1000, self.token).unwrap();
//!                }
//!                else {
//!                    ctrl.shutdown();
//!                }
//!            },
//!            EventType::Readable => {
//!                let mut buf : [u8; 5] = [0; 5];
//!                let num = self.sock.read(&mut buf).unwrap();
//!                let msg = String::from_utf8_lossy(&buf).into_owned();
//!                assert!(msg == "PONG!");
//!            },
//!            _ => {}
//!       }
//!    }
//!
//!    fn get_evented(&self) -> &Evented {
//!        &self.sock as &Evented
//!    }
//!
//!    fn get_interest(&self) -> Interest {
//!        self.interest
//!    }
//!}
//!fn main() {
//!    let mut client : Option<Token> = None;
//!    let mut server : Option<Token> = None;
//!    let mut r = Reactor::new();
//!
//!    //in order to listen, we must be prepared to create new connections
//!    //the handler for listen provides a socket a token, and the peer address
//!    //we then return an instance of Context of our choosing, in this case
//!    //we want to handle each inbound connection with an instance of ServConn
//!    let _ltoken = r.listen("127.0.0.1:10000", Box::new(|res, ctrl| {
//!        match res {
//!            ConnResult::Connected(sock, tok, addr) => {
//!                println!("Connection request from {}", addr);
//!                server = Some(tok);
//!
//!                //We've received a connection. Initiate PINGPONG protocol in 1 second
//!                ctrl.timeout_conn(1000, tok).unwrap();
//!
//!                Some(Box::new(ServConn {
//!                                interest: Interest::readable(),
//!                                token: tok.clone(),
//!                                sock: sock,
//!                                count: 0}))
//!            },
//!            _ => {panic!("We shouldn't be here")}
//!        }
//!    })).unwrap();
//!
//!    println!("Connecting to localhost");
//!
//!    //Like listen, connect requires that we specify how to create an instance of
//!    //Context when we successfully complete our connection. In this case we create
//!    //an instance of ClientConn
//!    r.connect("localhost", 10000, Box::new(|res, _ctrl| {
//!        match res {
//!            ConnResult::Connected(sock, tok, addr) => {
//!                println!("Completing connection to {}", addr);
//!                client = Some(tok);
//!                Some(Box::new(ClientConn {
//!                                interest: Interest::readable(),
//!                                token: tok.clone(),
//!                                sock: sock,
//!                              }))
//!            },
//!            ConnResult::Failed(err) => {panic!("Failed to connect to localhost:10000 error: {}", err)}
//!        }
//!    })).unwrap();
//!
//!    //block while running the event loop until it is terminated
//!    r.run();
//!
//!}
//!
//!```
//!
//!
#![feature(lookup_host)]

#[macro_use]
extern crate log;

extern crate mio;
extern crate iobuf;

mod context;
mod reactor;
mod reactor_ctrl;
mod reactor_handler;
pub mod utils;

pub use mio::{Interest, Evented, Token};
pub use mio::tcp;
pub use mio::udp;

pub use reactor::Reactor;
pub use context::{Context, EventType};

pub use reactor_ctrl::{ ReactorCtrl,
                        ConnHandler,
                        ConnResult,
                        TimeoutHandler,
                        ListenRec,
                        TimerRec};

