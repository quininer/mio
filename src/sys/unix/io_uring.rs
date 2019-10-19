use std::io;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::os::unix::io::RawFd;
use crate::{Interests, Token};
use crate::sys::Events;
use linux_io_uring::{opcode, squeue, IoUring};

#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};

/// Unique id for use as `SelectorId`.
#[cfg(debug_assertions)]
static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone)]
pub struct Selector {
    #[cfg(debug_assertions)]
    id: usize,
    ring: Arc<Mutex<IoUring>>
}

struct Data {
    fd: RawFd,
    token: Token,
    interests: Interests
}

impl Data {
    fn into_entry(self: Box<Self>) -> squeue::Entry {
        let mut entry = opcode::PollAdd::default();
        entry.fd = opcode::Target::Fd(self.fd);
        entry.mask = interests_to_poll(self.interests) as _;
        squeue::Entry::from(entry)
            .user_data(Box::into_raw(self) as _)
    }
}

impl Selector {
    pub fn new() -> io::Result<Selector> {
        let ring = IoUring::new(128)?;

        Ok(Selector {
            #[cfg(debug_assertions)]
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            ring: Arc::new(Mutex::new(ring))
        })
    }

    #[cfg(debug_assertions)]
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn try_clone(&self) -> io::Result<Selector> {
        Ok(self.clone())
    }

    pub fn select(&self, events: &mut Events, _timeout: Option<Duration>) -> io::Result<()> {
        events.clear();

        let mut ring = self.ring.lock().unwrap();

        ring.submit_and_wait(1)?;

        let mut queue = Vec::new();

        for entry in ring.completion().available() {
            let data = unsafe {
                Box::from_raw(entry.user_data() as *mut Data)
            };

            let event = Event {
                events: entry.result() as _,
                token: data.token.0 as _
            };

            queue.push(data);
            events.push(event);
        }

        let mut squeue = ring
            .submission()
            .available();

        for data in queue {
            unsafe {
                squeue.push(data.into_entry())
                    .map_err(|_| io::Error::new(io::ErrorKind::Other, "submission queue is full"))?;
            }
        }

        Ok(())
    }

    pub fn register(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        let entry = Box::new(Data { fd, token, interests })
            .into_entry();

        let mut ring = self.ring.lock().unwrap();

        unsafe {
            ring
                .submission()
                .available()
                .push(entry)
                .map_err(|_| io::Error::new(io::ErrorKind::Other, "submission queue is full"))?;
        }

        Ok(())
    }

    pub fn reregister(&self, fd: RawFd, token: Token, interests: Interests) -> io::Result<()> {
        self.register(fd, token, interests)
    }

    #[allow(unused_variables)]
    pub fn deregister(&self, fd: RawFd) -> io::Result<()> {
        // TODO

        Ok(())
    }
}

fn interests_to_poll(interests: Interests) -> i16 {
    let mut kind = 0;

    if interests.is_readable() {
        kind |= libc::POLLIN;
    }

    if interests.is_writable() {
        kind |= libc::POLLOUT;
    }

    kind
}

#[derive(Debug)]
pub struct Event {
    events: i16,
    token: u64
}

pub mod event {
    use crate::sys::Event;
    use crate::Token;

    pub fn token(event: &Event) -> Token {
        Token(event.token as usize)
    }

    pub fn is_readable(event: &Event) -> bool {
        (event.events & libc::POLLIN) != 0
            || (event.events & libc::POLLPRI) != 0
    }

    pub fn is_writable(event: &Event) -> bool {
        (event.events & libc::POLLOUT) != 0
    }

    pub fn is_error(event: &Event) -> bool {
        (event.events & libc::POLLERR) != 0
    }

    pub fn is_hup(event: &Event) -> bool {
        (event.events & libc::POLLHUP) != 0
    }

    pub fn is_read_hup(event: &Event) -> bool {
        // TODO libc::POLLRDHUP
        const POLLRDHUP: i16 = 0x2000;

        (event.events & POLLRDHUP) != 0
    }

    pub fn is_priority(event: &Event) -> bool {
        (event.events & libc::POLLPRI) != 0
    }

    pub fn is_aio(_: &Event) -> bool {
        // Not supported in the kernel, only in libc.
        false
    }

    pub fn is_lio(_: &Event) -> bool {
        // Not supported.
        false
    }
}
