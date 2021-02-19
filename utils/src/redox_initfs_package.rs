use std::convert::{TryFrom, TryInto};
use std::fs::{DirEntry, File, FileType, OpenOptions, ReadDir};
use std::path::{Path, PathBuf};

use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{FileExt, FileTypeExt};

use anyhow::{anyhow, Context, Result};
use clap::{App, Arg};

use redox_initfs::types as initfs;

const DEFAULT_MAX_SIZE: u64 = 8 * 1024 * 1024;

enum Entry {
    File(File),
    Dir(Dir),
}
struct Child {
    name: Vec<u8>,
    entry: Entry,
}
struct Dir {
    children: Vec<Child>,
}

struct State {
    file: File,
    offset: u64,
    max_size: u64,
    inode_count: u16,
}

fn read_directory(state: &mut State, path: &Path) -> Result<Dir> {
    let read_dir = path.read_dir().map_err(|error| {
        anyhow!(
            "failed to read directory `{}`: {}",
            path.to_string_lossy(),
            error
        )
    })?;

    let children = read_dir
        .map(|result| {
            let entry = result.map_err(|error| {
                anyhow!(
                    "failed to get a directory entry from `{}`: {}",
                    path.to_string_lossy(),
                    error
                )
            })?;

            let file_type = entry.file_type().map_err(|error| {
                anyhow!(
                    "failed to determine file type for `{}`: {}",
                    entry.path().to_string_lossy(),
                    error
                )
            })?;

            let unsupported_type = |ty: &str, entry: &DirEntry| {
                Err(anyhow!(
                    "failed to include {} at `{}`: not supported by redox-initfs",
                    ty,
                    entry.path().to_string_lossy()
                ))
            };
            let name = entry.path().into_os_string().into_vec();
            let entry = if file_type.is_symlink() {
                return unsupported_type("symlink", &entry);
            } else if file_type.is_socket() {
                return unsupported_type("socket", &entry);
            } else if file_type.is_fifo() {
                return unsupported_type("FIFO", &entry);
            } else if file_type.is_block_device() {
                return unsupported_type("block device", &entry);
            } else if file_type.is_char_device() {
                return unsupported_type("character device", &entry);
            } else if file_type.is_dir() {
                Entry::File(File::open(&entry.path()).map_err(|error| {
                    anyhow!(
                        "failed to open file `{}`: {}",
                        entry.path().to_string_lossy(),
                        error
                    )
                })?)
            } else if file_type.is_file() {
                Entry::Dir(read_directory(state, &entry.path())?)
            } else {
                return Err(anyhow!(
                    "unknown file type at `{}`",
                    entry.path().to_string_lossy()
                ));
            };

            // TODO: Allow the user to specify a lower limit than u16::MAX.
            state.inode_count = state.inode_count.checked_add(1)
                .ok_or_else(|| anyhow!("exceeded the maximum inode limit"))?;

            Ok(Child { entry, name })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Dir { children })
}

fn bump_alloc(state: &mut State, size: u64) -> Result<u64> {
    if state.offset + size <= state.max_size {
        let offset = state.offset;
        state.offset += size;
        Ok(offset)
    } else {
        Err(anyhow!("Bump allocation failed: max limit reached"))
    }
}

fn main() -> Result<()> {
    let matches = App::new("redox_initfs_package")
        .help("Package a Redox initfs")
        .arg(
            Arg::with_name("MAX_SIZE")
                .long("--max-size")
                .short("-m")
                .takes_value(true)
                .required(false)
                .help("Set the upper limit for how large the image can become (default 8 MiB)."),
        )
        .arg(
            Arg::with_name("SOURCE")
                .takes_value(true)
                .required(true)
                .help("Specify the source directory to build the image from."),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .takes_value(true)
                .required(true)
                .long("--output")
                .short("-o")
                .help("Specify the path of the new image file."),
        )
        .get_matches();

    let max_size = if let Some(max_size_str) = matches.value_of("MAX_SIZE") {
        max_size_str
            .parse::<u64>()
            .context("expected an integer for MAX_SIZE")?
    } else {
        DEFAULT_MAX_SIZE
    };

    let source = matches
        .value_of("SOURCE")
        .expect("expected the required arg SOURCE to exist");

    let destination = matches
        .value_of("OUTPUT")
        .expect("expected the required arg OUTPUT to exist");

    let destination_path = Path::new(destination);

    let previous_extension = destination_path.extension().map_or("", |ext| {
        ext.to_str()
            .expect("expected destination path to be valid UTF-8")
    });

    if !destination_path
        .metadata()
        .map_or(true, |metadata| metadata.is_file())
    {
        return Err(anyhow!("Destination file must be a file"));
    }

    let destination_temp_path =
        destination_path.with_extension(format!("{}.partial", previous_extension));

    let destination_temp_file = OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .create_new(false)
        .open(destination_temp_path)
        .context("failed to open destination file")?;

    let mut state = State {
        file: destination_temp_file,
        offset: 0,
        max_size,
        inode_count: 0,
    };

    let root = read_directory(&mut state, Path::new(source)).context("failed to read root")?;

    // NOTE: The header is always stored at offset zero.
    let header_offset = bump_alloc(
        &mut state,
        std::mem::size_of::<initfs::Header>().try_into()
            .expect("expected header size to fit"),
    )?;

    let inode_table_length = {
        let inode_entry_size: u64 = std::mem::size_of::<initfs::DirEntry>()
            .try_into()
            .expect("expected table entry size to fit");

        inode_entry_size
            .checked_mul(u64::from(state.inode_count))
            .ok_or_else(|| anyhow!("inode table too large"))?
    };

    let inode_table_offset = bump_alloc(
        &mut state,
        inode_table_length,
    )?;
    let inode_table_offset = initfs::Offset(
        u32::try_from(inode_table_offset)
            .map_err(|_| anyhow!("inode table located too far away"))?
    );

    let current_system_time = std::time::SystemTime::now();

    let time_since_epoch = current_system_time
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .context("could not calculate timestamp")?;

    let header = initfs::Header {
        magic: initfs::Magic(initfs::MAGIC),
        creation_time: initfs::Timespec {
            sec: time_since_epoch.as_secs(),
            nsec: time_since_epoch.subsec_nanos(),
        },
        inode_count: state.inode_count,
        inode_table_offset,
    };

    Ok(())
}
