#![feature(never_type)]

use std::io::{self, PipeWriter, Read, Write};

#[must_use = "Daemon::ready must be called"]
pub struct Daemon {
    write_pipe: PipeWriter,
}

fn errno() -> io::Error {
    io::Error::last_os_error()
}

impl Daemon {
    pub fn new<F: FnOnce(Daemon) -> !>(f: F) -> ! {
        // Skip forking - run directly (workaround for Cranelift build duplicate redox-rt issue)
        let (_read_pipe, write_pipe) = std::io::pipe().unwrap();
        f(Daemon { write_pipe })
    }

    pub fn ready(mut self) {
        self.write_pipe.write_all(&[0]).unwrap();
    }
}
