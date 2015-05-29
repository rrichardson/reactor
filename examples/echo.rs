
extern crate reactor;

use reactor::{ReactorCtrl,
              Reactor,
              ConnResult,
              Evented,
              Interest,
              Context,
              EventType,
              Token};

use reactor::tcp::{TcpListener, TcpStream};

struct EchoConn {
    interest: Interest,
    sock : TcpStream,
    token : Token
}

impl Context for EchoConn {

    fn on_event(&mut self, ctrl : &mut ReactorCtrl, evt : EventType) {
        match evt {
            EventType::Readable => {
            },
            EventType::Writable => {
            },
            EventType::Timeout(tid) => {
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

    let ltoken = r.listen("127.0.0.1:10000", Box::new(|res| {
        match res {
            ConnResult::Connected(sock, tok, addr) => {
                println!("Connection request from {}", addr);
                server = Some(tok);

                r.timeout_conn(1000, tok).unwrap();

                Some(Box::new(EchoConn {
                                interest: Interest::readable(),
                                token: tok,
                                sock: sock }))
            },
            _ => {panic!("We shouldn't be here")}
        }
    })).unwrap();

    r.connect("localhost", 10000, Box::new(|res| {
        match res {
            ConnResult::Connected(sock, tok, addr) => {
                client = Some(tok);
                Some(Box::new(EchoConn {
                                interest: Interest::readable(),
                                token: tok,
                                sock: sock }))
            },
            ConnResult::Failed(err) => {panic!("Failed to connect to localhost:10000 error: {}", err)}
        }
    })).unwrap();

    r.run();

}

