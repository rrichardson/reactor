
[![crates.io](https://img.shields.io/crates/v/reactor.svg)](https://crates.io/crates/reactor/)

[![Build Status](https://travis-ci.org/rrichardson/reactor.svg?branch=master)](https://travis-ci.org/rrichardson/reactor)

[API DOCS](http://rrichardson.github.io/reactor/)

## Reactor ##

A high performance, cross platform that makes it easy to do event-driven network programming.


Reactor is a thin wrapper around mio whose primary goal is to unify the event loops of various networked libraries.
The goal is to be able to handle the network event management of a web server and a database client (and connection
pool) in the same event_loop.

As with most high level libraries, Reactor is fairly opinionated, you might want to check out [the design
principles](docs/design.md) to see if you agree with this approach. Or you can just check out [the examples](examples/)

As mentioned, it is a high performance, low overhead event manager, it allocates very little at runtime, (boxed handlers
at connection time), presently, a completely serial message passing benchmark over TCP can run 50,000 round-trips per
second on a local machine, that's about 20 microseconds seconds per round-trip.

Reator Contexts are the core abstraction around a socket or datagram receiver. The primary event handler features a
mailbox style interface, which matches across all of the events which might effect a socket.

API Docs including a simple example can be found [here](http://rrichardson.github.io/reactor/)
