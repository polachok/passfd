use std::os::unix::io::{RawFd, AsRawFd};
use std::os::unix::net::UnixListener;
use std::fs::File;
use passfd::FdPassingExt;

fn main() {
    let file = File::open("/etc/passwd").unwrap();
    let listener = UnixListener::bind("/tmp/test.sock").unwrap();
    let (stream, _) = listener.accept().unwrap();
    stream.send_fd(file.as_raw_fd()).unwrap();
}
