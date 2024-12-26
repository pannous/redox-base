use core::cell::RefCell;

use alloc::rc::Rc;
use hashbrown::hash_map::DefaultHashBuilder;
use hashbrown::{HashMap, HashSet};
use redox_rt::proc::FdGuard;
use redox_scheme::{CallerCtx, OpenResult, RequestKind, SchemeMut, SignalBehavior, Socket, V2};
use syscall::{Result, O_CREAT};

pub fn run(write_fd: usize) {
    let socket = Socket::<V2>::create("proc").expect("failed to open proc scheme socket");
    let mut scheme = ProcScheme::new();

    let _ = syscall::write(1, b"process manager started\n").unwrap();
    let _ = syscall::write(write_fd, &[0]);
    let _ = syscall::close(write_fd);

    loop {
        let RequestKind::Call(req) = (match socket
            .next_request(SignalBehavior::Restart)
            .expect("bootstrap: failed to read scheme request from kernel")
        {
            Some(req) => req.kind(),
            None => break,
        }) else {
            continue;
        };
        let resp = req.handle_scheme_mut(&mut scheme);

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
}
struct Thread {
    fd: FdGuard,
    // sig_ctrl: MmapGuard<...>
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ProcessId(usize);

struct ProcScheme {
    processes: HashMap<ProcessId, Process, DefaultHashBuilder>,
    process_groups: HashSet<ProcessId, DefaultHashBuilder>,
    sessions: HashSet<ProcessId, DefaultHashBuilder>,
}
impl ProcScheme {
    pub fn new() -> ProcScheme {
        ProcScheme {
            processes: HashMap::new(),
            process_groups: HashSet::new(),
            sessions: HashSet::new(),
        }
    }
}
impl SchemeMut for ProcScheme {
    fn xopen(&mut self, path: &str, flags: usize, ctx: &CallerCtx) -> Result<OpenResult> {

    }
}
