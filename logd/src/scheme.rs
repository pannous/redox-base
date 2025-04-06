use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::mem;
use std::sync::mpsc::{self, Sender};

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult};
use syscall::error::*;
use syscall::schemev2::NewFdFlags;

pub struct LogHandle {
    context: Box<str>,
    bufs: BTreeMap<usize, Vec<u8>>,
}

pub struct LogScheme {
    next_id: usize,
    output_tx: Sender<Vec<u8>>,
    handles: BTreeMap<usize, LogHandle>,
}

impl LogScheme {
    pub fn new(mut files: Vec<File>) -> Self {
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>();

        std::thread::spawn(move || {
            for line in output_rx {
                for file in &mut files {
                    let _ = file.write(&line);
                    let _ = file.flush();
                }
            }
        });

        LogScheme {
            next_id: 0,
            output_tx,
            handles: BTreeMap::new(),
        }
    }
}

impl SchemeSync for LogScheme {
    fn open(&mut self, path: &str, _flags: usize, _ctx: &CallerCtx) -> Result<OpenResult> {
        let id = self.next_id;
        self.next_id += 1;

        self.handles.insert(
            id,
            LogHandle {
                context: path.to_string().into_boxed_str(),
                bufs: BTreeMap::new(),
            },
        );

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        _buf: &mut [u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let _handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        // TODO

        Ok(0)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;

        let handle_buf = handle.bufs.entry(ctx.pid).or_insert_with(|| Vec::new());

        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];

            if handle_buf.is_empty() && !handle.context.is_empty() {
                handle_buf.extend_from_slice(handle.context.as_bytes());
                handle_buf.extend_from_slice(b": ");
            }

            handle_buf.push(b);

            if b == b'\n' {
                self.output_tx.send(mem::take(handle_buf)).unwrap();
            }

            i += 1;
        }

        Ok(i)
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        Ok(0)
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        let scheme_path = b"log:";

        let mut i = 0;
        while i < buf.len() && i < scheme_path.len() {
            buf[i] = scheme_path[i];
            i += 1;
        }

        let mut j = 0;
        let context_bytes = handle.context.as_bytes();
        while i < buf.len() && j < context_bytes.len() {
            buf[i] = context_bytes[j];
            i += 1;
            j += 1;
        }

        Ok(i)
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        //TODO: flush remaining data?

        Ok(())
    }
}

impl LogScheme {
    pub fn on_close(&mut self, id: usize) {
        self.handles.remove(&id);
    }
}
