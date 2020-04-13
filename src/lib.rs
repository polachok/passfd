//! `passfd` allows passing file descriptors between unrelated processes
//! using Unix sockets.
//!
//! Both tokio 0.1 and 0.2 are supported with `tokio_01` and `tokio_02`
//! features. Please note that these features rely on internal representation
//! of UnixStream and are unsafe.
//!
//! # Example usage
//! ## Process 1 (sender)
//! ```
//! use passfd::FdPassingExt;
//! use std::fs::File;
//! use std::os::unix::io::AsRawFd;
//! use std::os::unix::net::UnixListener;
//!
//! let file = File::open("/etc/passwd").unwrap();
//! let listener = UnixListener::bind("/tmp/test.sock").unwrap();
//! let (stream, _) = listener.accept().unwrap();
//! stream.send_fd(file.as_raw_fd()).unwrap();
//! ```
//! ## Process 2 (receiver)
//! ```
//! use passfd::FdPassingExt;
//! use std::fs::File;
//! use std::io::Read;
//! use std::os::unix::io::FromRawFd;
//! use std::os::unix::net::UnixStream;
//!
//! let stream = UnixStream::connect("/tmp/test.sock").unwrap();
//! let fd = stream.recv_fd().unwrap();
//! let mut file = unsafe { File::from_raw_fd(fd) };
//! let mut buf = String::new();
//! file.read_to_string(&mut buf).unwrap();
//! println!("{}", buf);
//! ```

use libc::{self, c_int, c_void, msghdr};
use std::io::{Error, ErrorKind};
use std::mem;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;

#[cfg(feature = "tokio_01")]
pub mod tokio_01;

// Support for tokio 0.2
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

// buffer must be aligned to header (See cmsg(3))
#[repr(C)]
union HeaderAlignedBuf {
    // CMSG_SPACE(mem::size_of::<c_int>()) = 24 (linux x86_64),
    // we leave some space just in case
    // TODO: use CMSPG_SPACE when it's const fn
    // https://github.com/rust-lang/rust/issues/64926
    buf: [libc::c_char; 256],
    align: libc::cmsghdr,
}

impl FdPassingExt for RawFd {
    fn send_fd(&self, fd: RawFd) -> Result<(), Error> {
        let mut dummy: c_int = 0;
        let msg_len = unsafe { libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as _ };
        let mut u = HeaderAlignedBuf { buf: [0; 256] };
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: mem::size_of_val(&dummy),
        };

        let msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: unsafe { u.buf.as_mut_ptr() as *mut c_void },
            msg_controllen: msg_len,
            msg_flags: 0,
        };

        unsafe {
            let hdr = libc::cmsghdr {
                cmsg_level: libc::SOL_SOCKET,
                cmsg_type: libc::SCM_RIGHTS,
                cmsg_len: libc::CMSG_LEN(mem::size_of::<c_int>() as u32) as _,
            };
            // https://github.com/rust-lang/rust-clippy/issues/2881
            #[allow(clippy::cast_ptr_alignment)]
            std::ptr::write_unaligned(libc::CMSG_FIRSTHDR(&msg), hdr);

            // https://github.com/rust-lang/rust-clippy/issues/2881
            #[allow(clippy::cast_ptr_alignment)]
            std::ptr::write_unaligned(
                libc::CMSG_DATA(u.buf.as_mut_ptr() as *const _) as *mut c_int,
                fd,
            );
        }

        let rv = unsafe { libc::sendmsg(*self, &msg, 0) };
        if rv < 0 {
            return Err(Error::last_os_error());
        }

        Ok(())
    }

    fn recv_fd(&self) -> Result<RawFd, Error> {
        let mut dummy: c_int = -1;
        let msg_len = unsafe { libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as _ };
        let mut u = HeaderAlignedBuf { buf: [0; 256] };
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: mem::size_of_val(&dummy),
        };
        let mut msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: unsafe { u.buf.as_mut_ptr() as *mut c_void },
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
                    // https://github.com/rust-lang/rust-clippy/issues/2881
                    #[allow(clippy::cast_ptr_alignment)]
                    let fd = std::ptr::read_unaligned(libc::CMSG_DATA(hdr) as *mut c_int);
                    if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0 {
                        return Err(Error::last_os_error());
                    }
                    Ok(fd)
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
    fn assert_sized() {
        let msg_len = unsafe { libc::CMSG_SPACE(mem::size_of::<c_int>() as u32) as usize };
        let u = HeaderAlignedBuf { buf: [0; 256] };
        assert!(msg_len < std::mem::size_of_val(&u));
    }

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
