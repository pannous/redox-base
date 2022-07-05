#[no_mangle]
pub unsafe extern "C" fn main(_: libc::c_int, _: *const *const libc::c_char) -> libc::c_int {
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
        let iter = || env.split(|c| *c == b'\0').filter(|var| !var.is_empty());
        let env_count = iter().count();

        let envp = libc::calloc(env_count + 1, core::mem::size_of::<*const u8>()) as *mut *const libc::c_char;
        if envp.is_null() { panic!("failed to allocate envp buf"); }

        for (idx, var) in iter().enumerate() {
            envp.add(idx).write(var.as_ptr() as *const libc::c_char);
        }

        envp
    };

    let name_ptr = b"initfs:bin/init\0".as_ptr() as *const libc::c_char;
    let argv = [name_ptr, core::ptr::null()];

    if libc::execve(name_ptr, argv.as_ptr(), envp) == -1 {
        libc::perror(b"Failed to execute init\0".as_ptr() as *const libc::c_char);
    }
    unreachable!()
}
