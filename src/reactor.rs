use std::time::duration::Duration;
use std::io::{Error, Result};
use std::mem;
use std::rc::Rc;
use std::cell::RefCell;

use mio::{Sender, Evented, EventLoop, EventLoopConfig, Token, TimerResult, Timeout};
use reactor_handler::{ReactorHandler};
use context::{ Context, EventType };
use reactor_ctrl::{ReactorCtrl,
                   ReactorConfig,
                   ReactorState,
                   TaggedBuf,
                   ConnHandler,
                   TimeoutHandler};



pub struct Reactor
{
    state: Rc<RefCell<ReactorState>>,
    handler: ReactorHandler,
    event_loop: EventLoop<ReactorHandler>
}

impl Reactor
{

    /// Construct a new Reactor with (hopefully) intelligent defaults
    pub fn new() -> Reactor {
        let config = ReactorConfig {
            out_queue_size: 524288,
            max_connections: 10240,
            timers_per_connection: 1,
            poll_timeout_ms: 100
        };

        Self::configured(config)
    }

    /// Construct a new engine with defaults specified by the user
    pub fn configured(cfg: ReactorConfig) -> Reactor {
        let eloop = EventLoop::configured(
                    Self::event_loop_config(
                        cfg.out_queue_size, cfg.poll_timeout_ms,
                        (cfg.max_connections * cfg.timers_per_connection))).unwrap();

        let state = Rc::new(RefCell::new(ReactorState::new(cfg)));

        Reactor { state: state,
                  event_loop: eloop,
                  handler: ReactorHandler { state : state }
        }
    }

    fn event_loop_config(queue_sz : usize, timeout: usize, timer_cap : usize) -> EventLoopConfig {
        EventLoopConfig {
            io_poll_timeout_ms: timeout,
            notify_capacity: queue_sz,
            messages_per_tick: 512,
            timer_tick_ms: 10,
            timer_wheel_size: 1_024,
            timer_capacity: timer_cap
        }
    }

    /// connect to the supplied hostname and port
    /// any data that arrives on the connection will be put into a Buf
    /// and sent down the supplied Sender channel along with the Token of the connection
    pub fn connect<'b>(&mut self,
                   hostname: &'b str,
                   port: usize,
                   handler: Box<ConnHandler>) -> Result<Token> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .connect(hostname, port, handler)
    }

    /// listen on the supplied ip address and port
    /// any new connections will be accepted and polled for read events
    /// all datagrams that arrive will be put into StreamBufs with their
    /// corresponding token, and added to the default outbound data queue
    /// this can be called multiple times for different ips/ports
    pub fn listen<'b>(&mut self,
                  addr: &'b str,
                  port: usize,
                  handler: Box<ConnHandler>) -> Result<Token> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .listen(addr, port, handler)
    }

    /// fetch the event_loop channel for notifying the event_loop of new outbound data
    pub fn channel(&self) -> Sender<TaggedBuf> {
        self.event_loop.channel()
    }

    /// Set a timeout to be executed by the event loop after duration milliseconds
    /// The supplied handler, which is a FnMut will be invoked no sooner than the
    /// timeout
    pub fn timeout(&mut self, duration: u64, handler: Box<TimeoutHandler>) -> TimerResult<(Timeout, Token)> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .timeout(duration, handler)
    }

    /// Set a timeout to be executed by the event loop after duration milliseconds
    /// ctxtok specifies a Context to which the timer callback will be directed
    /// through the usual event dispatch mechanism for Contexts
    /// This is useful for handling protocols which have a ping/pong style timeout
    pub fn timeout_conn(&mut self, duration: u64, ctxtok: Token) -> TimerResult<(Timeout, Token)> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .timeout_conn(duration, ctxtok)
    }

    /// Trade in an existing context (connected to a resource) and get a Token
    /// The context will be registered for whichever events are specified in
    /// its own interest retrieved by get_interest()
    pub fn register(&mut self, ctx : Box<Context<Socket=Evented>>) -> Result<Token> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .register(ctx)
    }

    /// Trade in your token for a Context and deregister the Context's socket/evented
    /// from the event_loop
    pub fn deregister(&mut self, token: Token) -> Result<Box<Context<Socket=Evented>>> {
        ReactorCtrl::new(&mut (*self.state.borrow_mut()), &mut self.event_loop)
            .deregister(token)
    }

    /// process all incoming and outgoing events in a loop
    pub fn run(&mut self) {
        self.event_loop.run(&mut self.handler).map_err(|_| ()).unwrap();
    }

    /// process all incoming and outgoing events in a loop
    pub fn run_once(&mut self) {
        self.event_loop.run_once(&mut self.handler).map_err(|_| ()).unwrap();
    }

    /// calculates the 11th digit of pi
    pub fn shutdown(&mut self) {
        self.event_loop.shutdown();
    }
}

