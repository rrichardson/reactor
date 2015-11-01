#![feature(slice_bytes)]

/// This example will listen for incoming connections and, once connected, establish a ping/pong
/// session with them.  It will execute 3 pingpongs before shutting itself down.
/// Since this is an example, a couple things are simplified. One is simplification is that
/// it writes directly into a buffer wrather than using Buf fill/read-advance type semantics
/// which would be necessary if reads/writes didn't fully complete.

extern crate reactor;

use std::slice::bytes::copy_memory;
use std::string::String;
use std::io::{Read, Write};
use reactor::{ReactorCtrl,
              Reactor,
              ConnResult,
              Evented,
              EventSet,
              Context,
              EventType,
              Token};

use reactor::tcp::{TcpStream};

struct EchoConn {
    interest: EventSet,
    sock : TcpStream,
    token : Token,
    count : u32
}

impl Context for EchoConn {

    fn on_event(&mut self, ctrl : &mut ReactorCtrl, evt : EventType) {
        match evt {
            EventType::Readable => {
                let mut buf : [u8; 5] = [0; 5];
                let num = self.sock.read(&mut buf).unwrap();
                if num == 5 {
                    let msg = String::from_utf8_lossy(&buf).into_owned();
                    println!("Recieved : {}", msg);

                    if msg == "PING!" {
                        copy_memory("PONG!".as_bytes(), &mut buf);
                        self.sock.write(&buf).unwrap();
                    }
                    else if msg == "PONG!" {
                        println!("Successfully completed PING/PONG");
                        self.count += 1;
                        if self.count < 3 {
                            ctrl.timeout_conn(1000, self.token).unwrap();
                        }
                        else {
                            ctrl.shutdown();
                        }
                    }
                }
                else {
                    println!("only read {} bytes from socket", num);
                }
            },
            EventType::Timeout(_) => {
                let mut buf : [u8; 5] = [0; 5];
                copy_memory("PING!".as_bytes(), &mut buf);
                println!("Sending ping");
                self.sock.write(&buf).unwrap();
            },
            _ => {}
        }
    }

    fn get_evented(&self) -> &Evented {
        &self.sock as &Evented
    }

    fn get_interest(&self) -> EventSet {
        self.interest
    }
}

fn main() {
    let mut client : Option<Token> = None;
    let mut server : Option<Token> = None;
    let mut r = Reactor::new();

    let _ltoken = r.listen("127.0.0.1:10000", Box::new(|res, ctrl| {
        match res {
            ConnResult::Connected(sock, tok, addr) => {
                println!("Connection request from {}", addr);
                server = Some(tok);

                //We've received a connection. Initiate PINGPONG protocol in 1 second
                ctrl.timeout_conn(1000, tok).unwrap();

                Some(Box::new(EchoConn {
                                interest: EventSet::readable(),
                                token: tok.clone(),
                                sock: sock,
                                count: 0 }))
            },
            _ => {panic!("We shouldn't be here")}
        }
    })).unwrap();

    println!("Connecting to localhost");
    r.connect("localhost", 10000, Box::new(|res, _ctrl| {
        match res {
            ConnResult::Connected(sock, tok, addr) => {
                println!("Completing connection to {}", addr);
                client = Some(tok);
                Some(Box::new(EchoConn {
                                interest: EventSet::readable(),
                                token: tok.clone(),
                                sock: sock,
                                count: 0}))
            },
            ConnResult::Failed(err) => {panic!("Failed to connect to localhost:10000 error: {}", err)}
        }
    })).unwrap();

    r.run();

}

