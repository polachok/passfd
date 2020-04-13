use passfd::FdPassingExt;
use std::fs::File;
use std::io::Read;
use std::os::unix::io::FromRawFd;
use std::os::unix::net::UnixStream;

fn main() {
    let stream = UnixStream::connect("/tmp/test.sock").unwrap();
    let fd = stream.recv_fd().unwrap();
    let mut file = unsafe { File::from_raw_fd(fd) };
    let mut buf = String::new();
    file.read_to_string(&mut buf).unwrap();
    println!("{}", buf);
    std::thread::sleep(std::time::Duration::from_secs(30));
}
