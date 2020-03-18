//! `passfd` allows passing file descriptors between unrelated processes
//! using Unix sockets.

use libc::{self, c_int, c_void, msghdr};
use std::io::{Error, ErrorKind};
use std::mem;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;

#[cfg(feature = "tokio_01")]
pub mod tokio_01;

#[cfg(feature = "tokio_02")]
pub mod tokio_02;

/// Main trait, extends UnixStream
pub trait FdPassingExt {
    /// Send RawFd. No type information is transmitted.
    fn send_fd(&self, fd: RawFd) -> Result<(), Error>;
    /// Receive RawFd. No type information is transmitted.
    fn recv_fd(&self) -> Result<RawFd, Error>;
}

impl FdPassingExt for UnixStream {
    fn send_fd(&self, fd: RawFd) -> Result<(), Error> {
        self.as_raw_fd().send_fd(fd)
    }

    fn recv_fd(&self) -> Result<RawFd, Error> {
        self.as_raw_fd().recv_fd()
    }
}

impl FdPassingExt for RawFd {
    fn send_fd(&self, fd: RawFd) -> Result<(), Error> {
        let mut dummy: c_int = 0;
        let msg_len = unsafe { libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as _ };
        let mut buf = vec![0u8; msg_len as usize];
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: mem::size_of_val(&dummy),
        };
        unsafe {
            let hdr = libc::cmsghdr {
                cmsg_level: libc::SOL_SOCKET,
                cmsg_type: libc::SCM_RIGHTS,
                cmsg_len: libc::CMSG_LEN(mem::size_of::<c_int>() as u32) as _,
            };
            #[allow(clippy::cast_ptr_alignment)] // https://github.com/rust-lang/rust-clippy/issues/2881
            std::ptr::write_unaligned(buf.as_mut_ptr() as *mut _, hdr);

            #[allow(clippy::cast_ptr_alignment)] // https://github.com/rust-lang/rust-clippy/issues/2881
            std::ptr::write_unaligned(libc::CMSG_DATA(buf.as_mut_ptr() as *const _) as *mut c_int, fd);
        }
        let msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: buf.as_mut_ptr() as *mut c_void,
            msg_controllen: msg_len,
            msg_flags: 0,
        };

        let rv = unsafe { libc::sendmsg(*self, &msg, 0) };
        if rv < 0 {
            return Err(Error::last_os_error());
        }

        Ok(())
    }

    fn recv_fd(&self) -> Result<RawFd, Error> {
        let mut dummy: c_int = -1;
        let msg_len = unsafe { libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as _ };
        let mut buf = vec![0u8; msg_len as usize];
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: mem::size_of_val(&dummy),
        };
        let mut msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: buf.as_mut_ptr() as *mut c_void,
            msg_controllen: msg_len,
            msg_flags: 0,
        };

        unsafe {
            let rv = libc::recvmsg(*self, &mut msg, 0);
            match rv {
                0 => Err(Error::new(ErrorKind::UnexpectedEof, "0 bytes read")),
                rv if rv < 0 => Err(Error::last_os_error()),
                rv if rv == mem::size_of::<c_int>() as isize => {
                    let hdr: *mut libc::cmsghdr =
                        if msg.msg_controllen >= mem::size_of::<libc::cmsghdr>() as _ {
                            msg.msg_control as *mut libc::cmsghdr
                        } else {
                            return Err(Error::new(
                                ErrorKind::InvalidData,
                                "bad control msg (header)",
                            ));
                        };
                    if (*hdr).cmsg_level != libc::SOL_SOCKET || (*hdr).cmsg_type != libc::SCM_RIGHTS
                    {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            "bad control msg (level)",
                        ));
                    }
                    if msg.msg_controllen != libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as _ {
                        return Err(Error::new(ErrorKind::InvalidData, "bad control msg (len)"));
                    }
                    #[allow(clippy::cast_ptr_alignment)] // https://github.com/rust-lang/rust-clippy/issues/2881
                    let data = std::ptr::read_unaligned(libc::CMSG_DATA(hdr) as *mut c_int);
                    Ok(data)
                }
                _ => Err(Error::new(
                    ErrorKind::InvalidData,
                    "bad control msg (ret code)",
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use std::os::unix::io::{AsRawFd, FromRawFd};
    use std::os::unix::net::{UnixListener, UnixStream};
    use tempdir::TempDir;

    #[test]
    fn it_works() {
        let tmp_dir = TempDir::new("passfd").unwrap();
        let sock_path = tmp_dir.path().join("listener.sock");

        match unsafe { libc::fork() } {
            -1 => panic!("fork went wrong"),
            0 => {
                println!("child process, wait for socket to appear");
                for _ in 0..10 {
                    match UnixStream::connect(sock_path.clone()) {
                        Ok(stream) => {
                            println!("stream connected");
                            let fd = stream.recv_fd().unwrap();
                            let mut file = unsafe { File::from_raw_fd(fd) };
                            let mut buf = String::new();
                            file.read_to_string(&mut buf).unwrap();
                            return;
                        }
                        Err(_) => {
                            println!("not connected");
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            }
            _ => {
                println!("parent, start listening");
                let listener = UnixListener::bind(sock_path).unwrap();
                println!("opening file");
                let file = File::open("/etc/passwd").unwrap();
                let (stream, _) = listener.accept().unwrap();
                println!("client connected, sending fd");
                stream.send_fd(file.as_raw_fd()).unwrap();
            }
        }
    }
}
