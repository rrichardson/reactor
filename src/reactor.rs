use std::time::duration::Duration;
use std::io::Error;
use std::mem;
use mio::{Sender, Evented, EventLoop, EventLoopConfig, Token, TimerResult, Timeout};
use reactor_handler::{ReactorHandler};
use reactor_control::{ReactorCtrl, ReactorConfig, ReactorState, TaggedBuf};
use block_allocator::Allocator;
use protocol::Protocol;



pub struct Reactor
where
{
    ctrl: Rc<RefCell<ReactorCtrl,
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
        let ctrl = unsafe { ReactorCtrl::new(cfg,  mem::transmute(&mut eloop)) };
        rcontrol = Rc::new(RefCell::new(ctrl));
        Reactor { ctrl: rcontrol,
                  event_loop: eloop,
                  handler: ReactorHandler::new(rcontrol.clone())
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
                   hostname: &str,
                   port: usize) -> Result<Token, Error> {
        self.handler.ctrl.connect(hostname, port, &mut self.event_loop)
    }

    /// listen on the supplied ip address and port
    /// any new connections will be accepted and polled for read events
    /// all datagrams that arrive will be put into StreamBufs with their
    /// corresponding token, and added to the default outbound data queue
    /// this can be called multiple times for different ips/ports
    pub fn listen<'b, F : FnMut(sock : TcpStream)>(&mut self,
                  addr: &'b str,
                  port: usize,
                  cb : F) -> Result<Token, Error> {
        self.handler.listen(addr, port, &mut self.event_loop)
    }

    /// fetch the event_loop channel for notifying the event_loop of new outbound data
    pub fn channel(&self) -> Sender<TaggedBuf> {
        self.event_loop.channel()
    }

    /// Set a timeout to be executed by the event loop after duration
    /// Minimum expected resolution is the tick duration of the event loop
    /// poller, but it could be shorted depending on how many events are
    /// occurring
    pub fn timeout(&mut self, duration: u64, cid: u64) -> TimerResult<Timeout> {
        let tok = self.handler.timeouts.insert((cid, None)).map_err(|_| format!("failed")).unwrap();
        let handle = self.event_loop.timeout_ms(tok.0 as u64, duration).unwrap();
        self.handler.timeouts.get_mut(tok).unwrap().1 = Some(handle);
        Ok(handle)
    }

    pub fn register<E: Evented>(&mut self, io: &E, interest: Interest, opt: PollOpt) -> Result<()> {
        if let Some(token) = self.add_connection(io) {
            return event_loop.register_opt(io, token, interest, opt)
        }
        Err(Error::new(ErrorKind::Other, "Failed to add connection"))
    }

    pub fn deregister<E: Evented>(&mut self, io: &E) -> Result<()> {
        self.remove_connection(io);
        event_loop.deregister(io)
    }

    pub fn on_timer() -> Result<()> {
    }

    pub fn on_notify() -> Result<()> {
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

