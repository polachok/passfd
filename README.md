# passfd

[![Build Status](https://travis-ci.com/polachok/passfd.svg?branch=master)](https://travis-ci.com/polachok/passfd)

Unix sockets possess magic ability to transfer file descriptors from one process to another (unrelated) process using
obscure `SCM_RIGHTS` API. This little crate adds extension methods to [UnixStream](https://doc.rust-lang.org/std/os/unix/net/struct.UnixStream.html) to use it.

## Links

* [fd-passing](https://docs.rs/fd-passing/0.0.1/fd_passing/) same thing, different API
* [Good article on fd passing](https://keithp.com/blogs/fd-passing/)
