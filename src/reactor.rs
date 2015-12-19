use std::io::{Result};
use std::net::ToSocketAddrs;

use mio::{Sender, Evented, EventLoop, EventLoopConfig, Token, TimerResult, Timeout};
use reactor_handler::{ReactorHandler};
use context::{Context};
use reactor_ctrl::{ReactorCtrl,
                   ReactorConfig,
                   ReactorState,
                   TaggedBuf,
                   ConnHandler,
                   TimeoutHandler};

pub struct Reactor<'a>
{
    state: Option<ReactorState<'a>>,
    handler: ReactorHandler<'a>,
    event_loop: EventLoop<ReactorHandler<'a>>
}

impl<'a> Reactor<'a>
{

    /// Construct a new Reactor with (hopefully) intelligent defaults
    pub fn new() -> Reactor<'a> {
        let config = ReactorConfig {
            out_queue_size: 524288,
            max_connections: 10240,
            timers_per_connection: 1
        };

        Self::configured(config)
    }

    /// Construct a new engine with defaults specified by the user
    pub fn configured(cfg: ReactorConfig) -> Reactor<'a> {
        let eloop = EventLoop::configured(
                    Self::event_loop_config(
                        cfg.out_queue_size, (cfg.max_connections * cfg.timers_per_connection))).unwrap();

        let state = ReactorState::new(cfg);

        Reactor { state: Some(state),
                  event_loop: eloop,
                  handler: ReactorHandler { state : None }
        }
    }

    fn event_loop_config(queue_sz : usize, timer_cap : usize) -> EventLoopConfig {
        let mut foo = EventLoopConfig::new();
        foo.notify_capacity(queue_sz).
            messages_per_tick(512).
            timer_tick_ms(10).
            timer_wheel_size(1_024).
            timer_capacity(timer_cap);
        return foo;
    }

    /// Attempt a connection to the remote host specified at the remote hostname or ip address
    /// and the port.  This is a connection on a non-blocking socket, so the connect call will
    /// return immediately.  It requires a handler, to which it will supply a `ConnResult` which
    /// will indicate success or failure. On success, it will supply a socket, a token, and a
    /// remote IP addr. It then expects an Option<Box<`Context`>> so that it can manage its events
    pub fn connect<'b>(&mut self,
                   hostname: &'b str,
                   port: usize,
                   handler: Box<ConnHandler<'a>>) -> Result<Token> {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .connect(hostname, port, handler)
    }

    /// Listen on the supplied IP address:port for incoming TCP connections.  This returns
    /// immediately and expects a handler to which it will supply `ConnResult` and expect
    /// Option<Box<`Context`>> as a result
    pub fn listen<A : ToSocketAddrs>(&mut self,
                  addr: A,
                  handler: Box<ConnHandler<'a>>) -> Result<Token> {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .listen(addr, handler)
    }

    /// fetch the event_loop channel for notifying the event_loop of new outbound data
    pub fn channel(&self) -> Sender<TaggedBuf> {
        self.event_loop.channel()
    }

    /// Set a timeout to be executed by the event loop after duration milliseconds
    /// The supplied handler, which is a FnMut will be invoked no sooner than the
    /// timeout
    pub fn timeout(&mut self, duration: u64, handler: Box<TimeoutHandler<'a>>) -> TimerResult<(Timeout, Token)> {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .timeout(duration, handler)
    }

    /// Set a timeout to be executed by the event loop after duration milliseconds
    /// ctxtok specifies a Context to which the timer callback will be directed
    /// through the usual event dispatch mechanism for `Context`s
    /// This is useful for handling protocols which have a ping/pong style timeout
    pub fn timeout_conn(&mut self, duration: u64, ctxtok: Token) -> TimerResult<(Timeout, Token)> {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .timeout_conn(duration, ctxtok)
    }

    /// Trade in an existing context (connected to a resource) and get a Token
    /// The context will be registered for whichever events are specified in
    /// its own interest retrieved by get_interest()
    pub fn register<C>(&mut self, ctx : C) -> Result<Token>
    where C : Context + 'static
    {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .register(ctx)
    }

    /// Trade in your token for a Context and deregister the Context's socket/evented
    /// from the event_loop
    pub fn deregister(&mut self, token: Token) -> Result<Box<Context>>
    {
        ReactorCtrl::new(self.state.as_mut().unwrap(), &mut self.event_loop)
            .deregister(token)
    }

    /// process all incoming and outgoing events in a loop
    pub fn run(&mut self) {
        self.handler.state = self.state.take();
        self.event_loop.run(&mut self.handler).map_err(|_| ()).unwrap();
        self.state = self.handler.state.take();
    }

    /// process all incoming and outgoing events in a loop
    pub fn run_once(&mut self) {
        self.handler.state = self.state.take();
        self.event_loop.run_once(&mut self.handler, None).map_err(|_| ()).unwrap();
        self.state = self.handler.state.take();
    }

    /// calculates the 11th digit of pi
    pub fn shutdown(&mut self) {
        self.event_loop.shutdown();
    }
}

