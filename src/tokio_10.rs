//! Support for tokio 0.2 UnixStream.
//! It does a really bad `mem::transmute`, which is *NOT SAFE*

use std::future::Future;
use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures_core::ready;

use tokio1::io::Interest;
use tokio1::net::UnixStream;

use crate::FdPassingExt as SyncFdPassingExt;

/// Main trait, extends UnixStream
pub trait FdPassingExt {
    /// Send RawFd. No type information is transmitted.
    fn send_fd(&self, fd: RawFd) -> SendFd;
    /// Receive RawFd. No type information is transmitted.
    fn recv_fd(&self) -> RecvFd;
}

pub struct SendFd<'a> {
    stream: &'a UnixStream,
    fd: RawFd,
}

impl<'a> Future for SendFd<'a> {
    type Output = Result<(), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        let stream_fd = this.stream.as_raw_fd();

        ready!(this.stream.poll_write_ready(cx))?;

        let res = this
            .stream
            .try_io(Interest::WRITABLE, || stream_fd.send_fd(this.fd));
        match res {
            Ok(_) => Poll::Ready(Ok(())),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Poll::Pending,
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

pub struct RecvFd<'a> {
    stream: &'a UnixStream,
}

impl<'a> Future for RecvFd<'a> {
    type Output = Result<RawFd, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let this = Pin::into_inner(self);
        let stream_fd = this.stream.as_raw_fd();

        ready!(this.stream.poll_read_ready(cx))?;

        let res = this
            .stream
            .try_io(Interest::READABLE, || stream_fd.recv_fd());

        match res {
            Ok(val) => Poll::Ready(Ok(val)),
            Err(err) if err.kind() == ErrorKind::WouldBlock => Poll::Pending,
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

impl FdPassingExt for UnixStream {
    fn send_fd(&self, fd: RawFd) -> SendFd {
        SendFd { stream: self, fd }
    }

    fn recv_fd(&self) -> RecvFd {
        RecvFd { stream: self }
    }
}
