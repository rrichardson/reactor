
Reactor intends to be a flexible and generic event poller.  It is a thin wrapper around mio which simplifies the handler
interface into callbacks that can be made specific to each connection.  Connections/FDs/Sockets can 
be managed by Reactor regardless of whether they originate within or without mio,
as long as the item can be made to fit in a mio::Evented trait.

The design of this API is guided by the following principles (and opinions)

* Reactor should manage events for any number of heterogenous protocols and socket types. e.g. You should be able to use
  the same event loop for both an HTTP server as well as a database client at the same time.
* It makes no distinction between client and server. Most everything these days both listens() and connects() so these
  abstractions do as well.
* Reactor should be as simple as possible. Avoid code gymnastics in favor of uglier approaches that keep code smaller
  (read: This API uses trait objects for polymorphism :) )
* Reactor should offer the same API interface to the user regardless of the context. E.g. the outer API is the same as
  that offered via the event callback.
* Traits over Closures where possible.  As of the time of this writing, it is still easier to manage mutable references
  across multiple functions if they are members of a struct. If you really want to use callbacks, there will be a
  generic Context trait implementation which leverages closures.
* Sometimes Closures are more convenient. Callbacks that create Contexts are closures.
* Frameworks suck. - Unfortunately with a reactor model, the event_loop needs to drive the control flow. However The authortries to place as much useful functionality into utility libs, reducing the
  footprint of the core framework as much as possible.

There are two major constructs in Reactor, the Reactor itself, which contains the event loop and is the origination of
callbacks.  Contexts are registered with Reactor and the Context::on_event is called when the underlying poller detects
events for the socket contained within the context. 

Context is a trait which must be implemented for each type of callback/socket the user wishes to handle. The user also
registers interest within the context instance as well. 

Reactor also offers additional callback interfaces,  one for timer, one for on_accept, and one for notify. 
on_notify is called when the event loop detects messages on the event_loop::notify channel. 

on_timeout is called in response to a event_loop::timeout event. 

on_accept is called in response to a connection event if the user has chosen to listen for inbound TCP connections with
Reactor::listen.


