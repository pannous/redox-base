#[no_mangle]
pub unsafe extern "C" fn main(_: libc::c_int, _: *const *const libc::c_char) -> libc::c_int {
    let (initfs_offset, initfs_length);

    let envp = {
        let len = 4096;
        let buf = libc::calloc(len, 1);
        if buf.is_null() { panic!("failed to allocate env buf"); }
        let env = core::slice::from_raw_parts_mut(buf as *mut u8, len);

        let fd = syscall::open("sys:env", syscall::O_RDONLY | syscall::O_CLOEXEC).expect("bootstrap: failed to open env");
        let bytes_read = syscall::read(fd, env).expect("bootstrap: failed to read env");

        if bytes_read >= len {
            // TODO: Handle this, we can allocate as much as we want in theory.
            panic!("env is too large");
        }
        let env = &mut env[..bytes_read];

        for c in &mut *env {
            if *c == b'\n' {
                *c = b'\0';
            }
        }
        let raw_iter = || env.split(|c| *c == b'\0').filter(|var| !var.is_empty());

        let mut initfs_offset_opt = None;
        let mut initfs_length_opt = None;

        for var in raw_iter() {
            let equal_sign = var.iter().position(|c| *c == b'=').expect("malformed environment variable");
            let name = &var[..equal_sign];
            let value = &var[equal_sign + 1..];

            match name {
                b"INITFS_OFFSET" => initfs_offset_opt = core::str::from_utf8(value).ok().and_then(|s| usize::from_str_radix(s, 16).ok()),
                b"INITFS_LENGTH" => initfs_length_opt = core::str::from_utf8(value).ok().and_then(|s| usize::from_str_radix(s, 16).ok()),

                _ => continue,
            }
        }
        initfs_offset = initfs_offset_opt.expect("missing INITFS_OFFSET");
        initfs_length = initfs_length_opt.expect("missing INITFS_LENGTH");

        let iter = || raw_iter().filter(|var| !var.starts_with(b"INITFS_"));
        let env_count = iter().count();

        let envp = libc::calloc(env_count + 1, core::mem::size_of::<*const u8>()) as *mut *const libc::c_char;
        if envp.is_null() { panic!("failed to allocate envp buf"); }

        for (idx, var) in iter().enumerate() {
            envp.add(idx).write(var.as_ptr() as *const libc::c_char);
        }

        envp
    };
    {
        use syscall::flag::MapFlags;
        // XXX: It may be a little unsafe to mprotect this after relibc has started, but since only
        // the bootloader can influence the data we use, it should be fine security-wise.
        let _ = syscall::mprotect(initfs_offset, initfs_length, MapFlags::PROT_READ | MapFlags::MAP_PRIVATE).expect("mprotect failed for initfs");

        spawn_initfs(initfs_offset, initfs_length);
    }

    let name_ptr = b"initfs:bin/init\0".as_ptr() as *const libc::c_char;
    let argv = [name_ptr, core::ptr::null()];

    if libc::execve(name_ptr, argv.as_ptr(), envp) == -1 {
        libc::perror(b"Failed to execute init\0".as_ptr() as *const libc::c_char);
        panic!();
    }
    unreachable!()
}

unsafe fn spawn_initfs(initfs_start: usize, initfs_length: usize) {
    let mut buf = [0; 2];
    syscall::pipe2(&mut buf, syscall::O_CLOEXEC).expect("failed to open sync pipe");
    let [read, write] = buf;

    match libc::fork() {
        -1 => {
            libc::perror(b"Failed to fork in order to start initfs daemon\0".as_ptr() as *const libc::c_char);
            panic!();
        }
        // Continue serving the scheme as the child.
        0 => {
            let _ = syscall::close(read);
        }
        // Return in order to execute init, as the parent.
        _ => {
            let _ = syscall::close(write);
            let _ = syscall::read(read, &mut [0]);

            let _ = syscall::chdir("initfs:");

            return;
        }
    }
    crate::initfs::run(core::slice::from_raw_parts(initfs_start as *const u8, initfs_length), write);
}
