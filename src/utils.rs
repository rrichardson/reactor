

fn drain_write_queue {
        if let Some(&mut (ref mut proto, ref mut c)) = self.conns.get_mut(token) {

            let mut writable = true;
            while writable && c.outbuf.len() > 0 {
                let (result, sz, mid) = {
                    let (buf, mid) = c.front_mut().unwrap(); //shouldn't panic because of len() check
                    let sz = buf.len();
                    (self.sock.write(&mut OutBuf(*buf.0)), sz as usize, mid)
                };
                match result {
                    Ok(Some(n)) =>
                    {
                        debug!("Wrote {:?} out of {:?} bytes to socket", n, sz);
                        if n == sz {
                            self.outbuf.pop_front(); // we have written the contents of this buffer so lets get rid of it
                            if let Some(msg) = proto.on_write(mid) {
                                self.dispatch(msg):
                            }
                        }
                    },
                    Ok(None) => { // this is also very unlikely, we got a writable message, but failed
                        // to write anything at all.
                        debug!("Got Writable event for socket, but failed to write any bytes");
                        writable = false;
                    },
                    Err(e) => { error!("error writing to socket: {:?}", e); writable = false }
                }
            }

            if self.outbuf.len() > 0 {
                c.interest.insert(Interest::writable());
                event_loop.reregister(&c.sock, token, c.interest, PollOpt::edge()).unwrap();
            }
        }

}
