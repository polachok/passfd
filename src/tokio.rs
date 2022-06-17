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

#[cfg(test)]
mod tests {
    use std::os::unix::{io::AsRawFd, net::UnixStream as OsUnixStream, prelude::FromRawFd};
    use tempdir::TempDir;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{UnixListener, UnixStream},
    };

    use super::FdPassingExt;

    const SOCKET_NAME: &str = "passfd_tokio_test.sock";

    #[tokio::test]
    async fn passfd_tokio_test() {
        let tmp_dir = TempDir::new("passfd").unwrap();

        let sock_path1 = tmp_dir.path().join(SOCKET_NAME);
        let sock_path2 = sock_path1.clone();

        println!("Start listening at: {:?}", sock_path1);
        let listener = UnixListener::bind(sock_path1).unwrap();

        let j1 = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();

            println!("Incoming peer connection");
            let (left, right) = OsUnixStream::pair().unwrap();

            println!("Sending peer fd");
            stream.send_fd(left.as_raw_fd()).await.unwrap();
            println!("Succesfullt sent peer fd");

            let mut peer_stream = UnixStream::from_std(right).unwrap();
            let mut buffer = [0u8; 4];

            println!("Reading data from the peer");
            assert!(peer_stream.read(&mut buffer).await.unwrap() == 4);

            println!("Message sent through a socket: {:?}", buffer);
        });

        let j2 = tokio::spawn(async move {
            println!("Connection to the sender");
            let stream = UnixStream::connect(sock_path2).await.unwrap();

            println!("Succesfully connected to the sender. Reading file descriptor");
            let fd = stream.recv_fd().await.unwrap();
            println!("Succesfully read file descriptor");

            let mut peer_stream =
                UnixStream::from_std(unsafe { OsUnixStream::from_raw_fd(fd) }).unwrap();

            println!("Sending data to the peer");
            let buffer: [u8; 4] = [0, 0, 0, 42];
            peer_stream.write(&buffer).await.unwrap();
            println!("Succesfully sent data to the peer");
        });

        tokio::try_join!(j1, j2).unwrap();
    }
}
