
Reactor intends to be a flexible and generic event poller.  It is a thin wrapper around mio which simplifies the handler
interface into callbacks that can be made specific to each connection.  Connections/FDs/Sockets can originate within or without mio, as long as the item can be made to fit in a mio::Evented trait.

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


