//! Networking primitives
//!
//! The types provided in this module are non-blocking by default and are
//! designed to be portable across all supported Mio platforms. As long as the
//! [portability guidelines] are followed, the behavior should be identical no
//! matter the target platform.
//!
//! [portability guidelines]: ../struct.Poll.html#portability

mod tcp;
mod udp;

use std::sync::Mutex;
pub use self::tcp::{TcpListener, TcpStream};
pub use self::udp::UdpSocket;

/// Hook when write or read
pub trait IOHook {
    /// a read hook when read some bytes
    fn read_hook(&mut self, len: usize);
    /// a write hook when write some bytes
    fn write_hook(&mut self, len: usize);
}

static mut IO_HOOK: Option<Box<&'static Mutex<IOHook>>> = None;

/// init io statistics, must call first if exists
pub fn init_io_hook(io_hook: Box<&'static Mutex<IOHook>>) {
    unsafe {
        IO_HOOK = Some(io_hook);
    }
}

fn read_hook(len: usize) {
    unsafe {
        if let Some(hook) = IO_HOOK.as_ref() {
            if let Ok(mut hook) = hook.lock() {
                hook.read_hook(len);
            }
        }
    }
}

fn write_hook(len: usize) {
    unsafe {
        if let Some(hook) = IO_HOOK.as_ref() {
            if let Ok(mut hook) = hook.lock() {
                hook.write_hook(len);
            }
        }
    }
}
