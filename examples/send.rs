use passfd::FdPassingExt;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixListener;

fn main() {
    let file = File::open("/etc/passwd").unwrap();
    let listener = UnixListener::bind("/tmp/test.sock").unwrap();
    let (stream, _) = listener.accept().unwrap();
    stream.send_fd(file.as_raw_fd()).unwrap();
}
