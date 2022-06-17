//! Support for tokio UnixStream.

use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};

use async_trait::async_trait;
use tokio::{io::Interest, net::UnixStream};

use crate::FdPassingExt as SyncFdPassingExt;

/// Main trait, extends UnixStream
#[async_trait]
pub trait FdPassingExt {
    /// Send RawFd. No type information is transmitted.
    async fn send_fd(&self, fd: RawFd) -> Result<(), Error>;
    /// Receive RawFd. No type information is transmitted.
    async fn recv_fd(&self) -> Result<RawFd, Error>;
}

#[async_trait]
impl FdPassingExt for UnixStream {
    async fn send_fd(&self, fd: RawFd) -> Result<(), Error> {
        loop {
            self.writable().await?;

            match self.try_io(Interest::WRITABLE, || self.as_raw_fd().send_fd(fd)) {
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                r => return r,
            }
        }
    }

    async fn recv_fd(&self) -> Result<RawFd, Error> {
        loop {
            self.readable().await?;

            match self.try_io(Interest::READABLE, || self.as_raw_fd().recv_fd()) {
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                r => return r,
            }
        }
    }
}
