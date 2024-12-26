use core::cell::RefCell;

use alloc::rc::Rc;
use alloc::vec::Vec;
use alloc::vec;

use hashbrown::hash_map::DefaultHashBuilder;
use hashbrown::{HashMap, HashSet};
use redox_rt::proc::FdGuard;
use redox_scheme::{
    CallerCtx, OpenResult, RequestKind, Response, Scheme, SendFdRequest, SignalBehavior, Socket,
};
use slab::Slab;
use syscall::schemev2::NewFdFlags;
use syscall::{Error, FobtainFdFlags, Result, EBADF, EBADFD, EEXIST, EINVAL, ENOENT, O_CREAT};

pub fn run(write_fd: usize) {
    let socket = Socket::create("proc").expect("failed to open proc scheme socket");
    let mut scheme = ProcScheme::new();

    let _ = syscall::write(1, b"process manager started\n").unwrap();
    let _ = syscall::write(write_fd, &[0]);
    let _ = syscall::close(write_fd);

    loop {
        let Some(req) = socket
            .next_request(SignalBehavior::Restart)
            .expect("bootstrap: failed to read scheme request from kernel")
        else {
            continue;
        };
        let resp = match req.kind() {
            RequestKind::Call(req) => req.handle_scheme(&mut scheme),
            RequestKind::SendFd(req) => scheme.on_sendfd(&socket, &req),
            _ => continue,
        };

        if !socket
            .write_response(resp, SignalBehavior::Restart)
            .expect("bootstrap: failed to write scheme response to kernel")
        {
            break;
        }
    }

    unreachable!()
}

struct Process {
    threads: Vec<Rc<RefCell<Thread>>>,
    ppid: ProcessId,
    pgid: ProcessId,
    sid: ProcessId,

    ruid: u32,
    euid: u32,
    rgid: u32,
    egid: u32,
    rns: u32,
    ens: u32,
}
struct Thread {
    fd: FdGuard,
    // sig_ctrl: MmapGuard<...>
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProcessId(usize);

const INIT_PID: ProcessId = ProcessId(1);

struct ProcScheme {
    processes: HashMap<ProcessId, Process, DefaultHashBuilder>,
    process_groups: HashSet<ProcessId, DefaultHashBuilder>,
    sessions: HashSet<ProcessId, DefaultHashBuilder>,
    handles: Slab<Handle>,

    init_claimed: bool,
    next_id: ProcessId,
}

enum Handle {
    Init,
    Proc(ProcessId),
}

impl ProcScheme {
    pub fn new() -> ProcScheme {
        ProcScheme {
            processes: HashMap::new(),
            process_groups: HashSet::new(),
            sessions: HashSet::new(),
            handles: Slab::new(),
            init_claimed: false,
            next_id: ProcessId(2),
        }
    }
    fn new_id(&mut self) -> ProcessId {
        let id = self.next_id;
        self.next_id.0 += 1;
        id
    }
    fn on_sendfd(&mut self, socket: &Socket, req: &SendFdRequest) -> Response {
        match self.handles[req.id()] {
            ref mut st @ Handle::Init => {
                let mut fd_out = usize::MAX;
                if let Err(e) = req.obtain_fd(socket, FobtainFdFlags::empty(), Err(&mut fd_out)) {
                    return Response::for_sendfd(&req, Err(e));
                };
                let thread = Rc::new(RefCell::new(Thread {
                    fd: FdGuard::new(fd_out),
                }));
                self.processes.insert(
                    INIT_PID,
                    Process {
                        threads: vec![thread],
                        ppid: INIT_PID,
                        sid: INIT_PID,
                        pgid: INIT_PID,
                        ruid: 0,
                        euid: 0,
                        rgid: 0,
                        egid: 0,
                        rns: 1,
                        ens: 1,
                    },
                );
                self.process_groups.insert(INIT_PID);
                self.sessions.insert(INIT_PID);

                *st = Handle::Proc(INIT_PID);
                Response::for_sendfd(&req, Ok(0))
            }
            _ => Response::for_sendfd(&req, Err(Error::new(EBADF))),
        }
    }
    fn fork(&mut self, parent_pid: ProcessId) -> Result<ProcessId> {
        let child_pid = self.new_id();

        let Process {
            pgid,
            sid,
            euid,
            ruid,
            egid,
            rgid,
            ens,
            rns,
            ..
        } = *self.processes.get(&parent_pid).ok_or(Error::new(EBADFD))?;

        self.processes.insert(
            child_pid,
            Process {
                threads: Vec::new(),
                ppid: parent_pid,
                pgid,
                sid,
                ruid,
                rgid,
                euid,
                egid,
                rns,
                ens,
            },
        );
        Ok(child_pid)
    }
    fn new_thread(&mut self, pid: ProcessId) -> Result<FdGuard> {
        let proc = self.processes.get_mut(&pid).ok_or(Error::new(EBADFD))?;
        proc.threads
            .push(Rc::new(RefCell::new(Thread { fd: todo!() })));
    }
}
impl Scheme for ProcScheme {
    fn xopen(&mut self, path: &str, flags: usize, ctx: &CallerCtx) -> Result<OpenResult> {
        if path == "init" {
            if core::mem::replace(&mut self.init_claimed, true) {
                return Err(Error::new(EEXIST));
            }
            return Ok(OpenResult::ThisScheme {
                number: self
                    .handles
                    .insert(Handle::Init),
                flags: NewFdFlags::empty(),
            });
        }
        Err(Error::new(ENOENT))
    }
    fn xdup(&mut self, old_id: usize, buf: &[u8], ctx: &CallerCtx) -> Result<OpenResult> {
        match self.handles[old_id] {
            Handle::Proc(pid) => match buf {
                b"fork" => {
                    let child_pid = self.fork(pid)?;
                    Ok(OpenResult::ThisScheme {
                        number: self.handles.insert(Handle::Proc(child_pid)),
                        flags: NewFdFlags::empty(),
                    })
                },
                b"new-thread" => Ok(OpenResult::OtherScheme {
                    fd: self.new_thread(pid)?.take(),
                }),
                _ => return Err(Error::new(EINVAL)),
            },
            Handle::Init => Err(Error::new(EBADF)),
        }
    }
}
