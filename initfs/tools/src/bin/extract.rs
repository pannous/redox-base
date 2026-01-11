use std::path::Path;
use std::fs;
use redox_initfs::{InitFs, InodeKind, Inode};
use anyhow::{Context, Result};
use clap::{Arg, Command};

fn extract_inode(initfs: &InitFs, parent_path: &Path, inode: Inode) -> Result<()> {
    let inode_struct = initfs.get_inode(inode).context("inode not found")?;
    
    match inode_struct.kind() {
        InodeKind::Dir(dir) => {
            fs::create_dir_all(parent_path)?;
            let ec = dir.entry_count()?;
            for i in 0..ec {
                if let Some(entry) = dir.get_entry(i)? {
                    let name = String::from_utf8_lossy(entry.name()?);
                    let child_path = parent_path.join(&*name);
                    
                    let child = initfs.get_inode(entry.inode()).context("child not found")?;
                    match child.kind() {
                        InodeKind::Dir(_) => extract_inode(initfs, &child_path, entry.inode())?,
                        InodeKind::File(f) => {
                            fs::write(&child_path, f.data()?)?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let mode = child.mode();
                                fs::set_permissions(&child_path, fs::Permissions::from_mode(mode as u32))?;
                            }
                            eprintln!("Extracted: {}", child_path.display());
                        }
                        InodeKind::Link(l) => {
                            let target = String::from_utf8_lossy(l.data()?);
                            #[cfg(unix)]
                            std::os::unix::fs::symlink(&*target, &child_path)?;
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn main() -> Result<()> {
    let matches = Command::new("redox-initfs-extract")
        .about("extract initfs to directory")
        .version(clap::crate_version!())
        .arg(Arg::new("IMAGE").required(true).help("initfs image to extract"))
        .arg(Arg::new("OUTPUT").required(true).help("output directory"))
        .get_matches();

    let source = matches.get_one::<String>("IMAGE").unwrap();
    let output = matches.get_one::<String>("OUTPUT").unwrap();

    let bytes = fs::read(source).context("failed to read image")?;
    let initfs = InitFs::new(&bytes, None).context("failed to parse initfs")?;
    
    extract_inode(&initfs, Path::new(output), InitFs::ROOT_INODE)?;
    
    eprintln!("Extraction complete!");
    Ok(())
}
