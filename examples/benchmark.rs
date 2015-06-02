#![feature(core)]

/// See how quickly we can send 1,000,000 round trip pingpongs
/// in a purely serial fashion
extern crate time;
extern crate reactor;

use std::slice::bytes::copy_memory;
use std::string::String;
use std::io::{Read, Write};
use reactor::{ReactorCtrl,
              Reactor,
              ConnResult,
              Evented,
              Interest,
              Context,
              EventType,
              Token};

use reactor::tcp::{TcpStream};

use time::{precise_time_ns};

struct EchoConn {
    interest: Interest,
    sock : TcpStream,
    token : Token,
    count : u32,
    start_time : u64,
}

impl Context for EchoConn {

    fn on_event(&mut self, ctrl : &mut ReactorCtrl, evt : EventType) {
        match evt {
            EventType::Readable => {
                let mut buf : [u8; 5] = [0; 5];
                let num = self.sock.read(&mut buf).unwrap();
                if num == 5 {
                    let msg = String::from_utf8_lossy(&buf).into_owned();

                    if msg == "PING!" {
                        copy_memory("PONG!".as_bytes(), &mut buf);
                        self.sock.write(&buf).unwrap();
                    }
                    else if msg == "PONG!" {
                        self.count += 1;
                        if self.count < 1_000_000 {
                            copy_memory("PING!".as_bytes(), &mut buf);
                            self.sock.write(&buf).unwrap();
                        }
                        else {
                            let result = precise_time_ns() - self.start_time;
                            let elapsed_secs = result as f64 / 1_000_000_000_f64;
                            println!("Sent 1,000,000 messages in {:.4} seconds | {:.4} msgs/sec", elapsed_secs, 1_000_000_f64 / elapsed_secs);
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
                println!("Starting ping test");
                self.start_time = precise_time_ns();
                self.sock.write(&buf).unwrap();
            },
            _ => {}
        }
    }

    fn get_evented(&self) -> &Evented {
        &self.sock as &Evented
    }

    fn get_interest(&self) -> Interest {
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
                                interest: Interest::readable(),
                                token: tok.clone(),
                                sock: sock,
                                count: 0,
                                start_time: 0}))
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
                                interest: Interest::readable(),
                                token: tok.clone(),
                                sock: sock,
                                count: 0,
                                start_time: 0}))
            },
            ConnResult::Failed(err) => {panic!("Failed to connect to localhost:10000 error: {}", err)}
        }
    })).unwrap();

    r.run();

}


