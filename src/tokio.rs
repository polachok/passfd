//! Support for tokio 0.2 UnixStream.
//! It does a really bad `mem::transmute`, which is *NOT SAFE*

use std::future::Future;
use std::io::{Error, ErrorKind};
use std::os::unix::io::{AsRawFd, RawFd};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use futures_core::ready;

use tokio::io::Interest;
use tokio::net::UnixStream;

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

        loop {
            ready!(this.stream.poll_write_ready(cx))?;

            let res = this
                .stream
                .try_io(Interest::WRITABLE, || stream_fd.send_fd(this.fd));
            match res {
                Ok(_) => break Poll::Ready(Ok(())),
                Err(err) if err.kind() == ErrorKind::WouldBlock => continue,
                Err(err) => break Poll::Ready(Err(err)),
            }
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

        loop {
            ready!(this.stream.poll_read_ready(cx))?;

            let res = this
                .stream
                .try_io(Interest::READABLE, || stream_fd.recv_fd());

            match res {
                Ok(val) => break Poll::Ready(Ok(val)),
                Err(err) if err.kind() == ErrorKind::WouldBlock => continue,
                Err(err) => break Poll::Ready(Err(err)),
            }
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

#[cfg(test)]
mod tests {
    use super::FdPassingExt;
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd};
    use tempdir::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{UnixListener, UnixStream};
    use tokio::runtime::Builder;

    #[test]
    fn async_it_works() {
        let tmp_dir = TempDir::new("passfd").unwrap();
        let sock_path = tmp_dir.path().join("listener.sock");

        match unsafe { libc::fork() } {
            -1 => panic!("fork went wrong"),
            0 => {
                println!("child process, wait for socket to appear");
                let rt = Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    for _ in 0..10 {
                        match UnixStream::connect(sock_path.clone()).await {
                            Ok(mut stream) => {
                                println!("stream connected");
                                let fd = stream.recv_fd().await?;
                                println!("fd received");
                                let mut file = unsafe { File::from_raw_fd(fd) };
                                let mut buf = String::new();
                                file.read_to_string(&mut buf)?;
                                stream.write_all("ok".as_bytes()).await?;
                                break;
                            }
                            Err(e) => {
                                println!("not connected {:?}", e);
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            }
                        }
                    }
                    Ok::<_, std::io::Error>(())
                })
                .unwrap();
            }
            _ => {
                println!("parent, start listening");
                let rt = Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async {
                    let listener = UnixListener::bind(sock_path)?;
                    println!("opening file");
                    let file = File::open("/etc/passwd")?;
                    let (mut stream, _) = listener.accept().await?;
                    println!("client connected, sending fd");
                    stream.send_fd(file.as_raw_fd()).await?;
                    let mut buf = String::new();
                    stream.read_to_string(&mut buf).await?;
                    println!("got response {:?}", buf);
                    assert!(buf == "ok");
                    Ok::<_, std::io::Error>(())
                })
                .unwrap();
            }
        }
    }
}
