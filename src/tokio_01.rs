//! Support for tokio 0.1 UnixStream.
//! It does a really bad `mem::transmute`, which is *NOT SAFE*

use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};

use futures::Async;
use tokio_uds::UnixStream;

use crate::FdPassingExt as SyncFdPassingExt;
use mio::Ready;

/// Main trait, extends UnixStream
pub trait FdPassingExt {
    /// Send RawFd. No type information is transmitted.
    fn poll_send_fd(&self, fd: RawFd) -> Result<Async<()>, Error>;
    /// Receive RawFd. No type information is transmitted.
    fn poll_recv_fd(&self) -> Result<Async<RawFd>, Error>;
}

impl FdPassingExt for UnixStream {
    fn poll_send_fd(&self, fd: RawFd) -> Result<Async<()>, Error> {
        self.poll_write_ready()?;

        match self.as_raw_fd().send_fd(fd) {
            Ok(_) => Ok(Async::Ready(())),
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                unsafe { clear_write_ready(self)? };
                Ok(Async::NotReady)
            }
            Err(err) => Err(err),
        }
    }

    fn poll_recv_fd(&self) -> Result<Async<RawFd>, Error> {
        self.poll_read_ready(Ready::readable())?;

        match self.as_raw_fd().recv_fd() {
            Ok(val) => Ok(Async::Ready(val)),
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                unsafe { clear_read_ready(self, Ready::readable())? };
                Ok(Async::NotReady)
            }
            Err(err) => Err(err),
        }
    }
}

unsafe fn clear_read_ready(stream: &UnixStream, ready: Ready) -> Result<(), Error> {
    use tokio_reactor::PollEvented;
    let inner: &PollEvented<mio_uds::UnixStream> = std::mem::transmute(stream);
    inner.clear_read_ready(ready)
}

unsafe fn clear_write_ready(stream: &UnixStream) -> Result<(), Error> {
    use tokio_reactor::PollEvented;
    let inner: &PollEvented<mio_uds::UnixStream> = std::mem::transmute(stream);
    inner.clear_write_ready()
}
