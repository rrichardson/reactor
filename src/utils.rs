use std::io::Write;
use std::collections::VecDeque;
use tendril::{Tendril, Atomic};
use tendril::fmt::Bytes;

/// Simple manager of outbound data for a non-blocking socket
pub struct OutQueue {
    q : VecDeque<Tendril<Bytes, Atomic>>,
    offset : usize
}

impl OutQueue {

    /// Attempt to write data into non-blocking socket.
    /// If all data was successfully written, then return true,
    /// otherwise place remaining data in queue to be written at next
    /// writable event.  User of this function should set their interest
    /// to writable upon a false result of this function
    pub fn write<W : Write>(&mut self, buf : Tendril<Bytes, Atomic>, sock : &mut W) -> bool {
        let b = buf;
        if self.q.is_empty() {
            if let Ok(n) = sock.write(&b[self.offset .. ]) {
                self.offset += n;
                if b.len() <= self.offset {
                    return true;
                }
            }
        }
        self.q.push_back(b);
        false
    }

    /// Attempt to empty the existing write queue into this non-blocking socket
    /// If all data was successfully written, then return true,
    /// otherwise place remaining data in queue to be written at next
    /// writable event.  User of this function should set their interest
    /// to writable upon a false result of this function
    pub fn drain<W : Write>(&mut self, sock : &mut W) -> bool {
        let mut writable = true;
        while writable && !self.q.is_empty() {
            let mut flushed = false;
            {
                let buf = self.q.front_mut().unwrap(); //shouldn't panic because of is_empty() check
                let sz = buf.len();
                match sock.write(&buf[self.offset .. ]) {
                    Ok(n) =>
                    {
                        if n == 0 {
                            error!("Got Writable event for socket, but failed to write any bytes");
                            writable = false;
                        }
                        else if n == sz {
                            flushed = true;
                        } else {
                            self.offset += n;
                        }
                    },
                    Err(e) => { error!("error writing to socket: {:?}", e); writable = false }
                }
            }
            if flushed {
                self.q.pop_front(); // we have written the contents of this buffer so lets get rid of it
            }
        }

        if self.q.is_empty() {
            true
        }
        else {
            false
        }
    }
}
