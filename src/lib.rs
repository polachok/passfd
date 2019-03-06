/// File descriptor passing library
use std::os::unix::io::{RawFd, AsRawFd};
use std::os::unix::net::UnixStream;
use libc::{self, c_void, c_int, msghdr};

pub trait FdPassingExt {
    /// Send RawFd
    fn send_fd(&self, fd: RawFd) -> Result<(), std::io::Error>;
    /// Receive RawFd
    fn recv_fd(&self) -> Result<RawFd, std::io::Error>;
}

impl FdPassingExt for UnixStream {
    fn send_fd(&self, fd: RawFd) -> Result<(), std::io::Error> {
        let mut dummy: c_int = 0;
        let mut buf = vec![0u8; unsafe { libc::CMSG_SPACE(std::mem::size_of::<c_int>() as u32) as usize }];
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: std::mem::size_of_val(&dummy),
        };
        unsafe {
            let hdr: *mut libc::cmsghdr = buf.as_mut_ptr() as *mut libc::cmsghdr;
            (*hdr).cmsg_level = libc::SOL_SOCKET;
            (*hdr).cmsg_type = libc::SCM_RIGHTS;
            (*hdr).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<c_int>() as u32) as usize;
            let data = libc::CMSG_DATA(hdr) as *mut c_int;
            *data = fd;
        }
        let mut msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: buf.as_mut_ptr() as *mut c_void,
            msg_controllen: buf.len(),
            msg_flags: 0,
        };

        let rv = unsafe { libc::sendmsg(self.as_raw_fd(), &mut msg, 0) };
        if rv < 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }

    fn recv_fd(&self) -> Result<RawFd, std::io::Error> {
        let mut dummy: c_int = -1;
        let mut buf = vec![0u8; unsafe { libc::CMSG_SPACE(std::mem::size_of::<c_int>() as u32) as usize }];
        let mut iov = libc::iovec {
            iov_base: &mut dummy as *mut c_int as *mut c_void,
            iov_len: std::mem::size_of_val(&dummy),
        };
        let mut msg: msghdr = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: buf.as_mut_ptr() as *mut c_void,
            msg_controllen: buf.len(),
            msg_flags: 0,
        };

        unsafe {
            let rv = libc::recvmsg(self.as_raw_fd(), &mut msg, 0);
            if rv < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if rv == std::mem::size_of::<c_int>() as isize {
                let hdr: *mut libc::cmsghdr =
                    if msg.msg_controllen >= std::mem::size_of::<libc::cmsghdr>() {
                        msg.msg_control as *mut libc::cmsghdr
                    } else {
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad control msg"));
                    };
                if (*hdr).cmsg_level != libc::SOL_SOCKET || (*hdr).cmsg_type != libc::SCM_RIGHTS {
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad control msg"));
                }
                if msg.msg_controllen != libc::CMSG_SPACE(std::mem::size_of::<c_int>() as u32) as usize {
                        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad control msg"));
                }
                let data = libc::CMSG_DATA(hdr) as *mut c_int;
                return Ok(*data);
            } else {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "bad control msg"));
            }
        }
    }
}
