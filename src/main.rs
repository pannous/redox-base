extern crate syscall;

use std::{fs, io, mem, process, slice, thread};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use syscall::data::{Packet, SigAction};
use syscall::daemon::Daemon;
use syscall::flag::{SigActionFlags, SIGUSR1};
use syscall::scheme::SchemeBlockMut;

use self::scheme::AudioScheme;

mod scheme;

fn from_syscall_error(error: syscall::Error) -> io::Error {
    io::Error::from_raw_os_error(error.errno as i32)
}

extern "C" fn sigusr_handler(_sig: usize) {}

fn thread(scheme: Arc<Mutex<AudioScheme>>, pid: usize, mut hda_file: fs::File) -> io::Result<()> {
    // Enter null namespace
    syscall::setrens(0, 0).map_err(from_syscall_error)?;

    loop {
        let buffer = scheme.lock().unwrap().buffer();
        let buffer_u8 = unsafe {
            slice::from_raw_parts(
                buffer.as_ptr() as *const u8,
                mem::size_of_val(&buffer)
            )
        };

        // Wake up the scheme thread
        syscall::kill(pid, SIGUSR1).map_err(from_syscall_error)?;

        hda_file.write(&buffer_u8)?;
    }
}

fn daemon(daemon: Daemon) -> io::Result<()> {
    // Handle signals from the hda thread
    syscall::sigaction(SIGUSR1, Some(&SigAction {
        sa_handler: Some(sigusr_handler),
        sa_mask: [0; 2],
        sa_flags: SigActionFlags::empty(),
    }), None).map_err(from_syscall_error)?;

    let pid = syscall::getpid().map_err(from_syscall_error)?;

    let hda_file = fs::OpenOptions::new().write(true).open("hda:")?;

    let mut scheme_file = fs::OpenOptions::new().create(true).read(true).write(true).open(":audio")?;

    let scheme = Arc::new(Mutex::new(AudioScheme::new()));

    // Spawn a thread to mix and send audio data
    let scheme_thread = scheme.clone();
    let _thread = thread::spawn(move || thread(scheme_thread, pid, hda_file));

    // Enter the null namespace - done after thread is created so
    // memory: can be accessed for stack allocation
    syscall::setrens(0, 0).map_err(from_syscall_error)?;

    // The scheme is now ready to accept requests, notify the original process
    daemon.ready().map_err(from_syscall_error)?;

    let mut todo = Vec::new();
    loop {
        let mut packet = Packet::default();
        let count = match scheme_file.read(&mut packet) {
            Ok(ok) => ok,
            Err(err) => if err.kind() == io::ErrorKind::Interrupted {
                0
            } else {
                return Err(err);
            }
        };

        if count > 0 {
            if let Some(a) = scheme.lock().unwrap().handle(&mut packet) {
                packet.a = a;
                scheme_file.write(&packet)?;
            } else {
                todo.push(packet);
            }
        }

        let mut i = 0;
        while i < todo.len() {
            if let Some(a) = scheme.lock().unwrap().handle(&mut todo[i]) {
                let mut packet = todo.remove(i);
                packet.a = a;
                scheme_file.write(&packet)?;
            } else {
                i += 1;
            }
        }
    }
}

fn main() {
    if let Err(err) = Daemon::new(|x| {
        match daemon(x) {
            Ok(()) => {
                process::exit(0);
            },
            Err(err) => {
                eprintln!("audiod: {}", err);
                process::exit(1);
            }
        }
    }) {
        eprintln!("audiod: {}", err);
        process::exit(1);
    }
}
