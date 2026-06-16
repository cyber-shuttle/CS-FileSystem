// Read-only CS filesystem for exposing Cybershuttle data sources.
//
// The FUSE mode mounts pluggable data sources as a virtual filesystem. The
// materialize mode writes the same logical tree to disk for NFS export.

mod atlas;
mod nfs_server;

use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use libc::ENOENT;

const TTL: Duration = Duration::from_secs(1);
const ROOT_INO: u64 = 1;
const HELLO_INO: u64 = 2;
const HELLO_CONTENT: &str = "Hello, FUSE from Rust!\n";

pub trait VirtualDataSource {
    fn name(&self) -> &str;
    fn inode(&self) -> u64;

    fn lookup(&self, parent: u64, name: &OsStr) -> Option<FileAttr>;
    fn getattr(&self, ino: u64) -> Option<FileAttr>;
    fn read(&self, ino: u64, offset: i64, size: u32) -> Option<Vec<u8>>;
    fn readdir(&self, ino: u64, offset: i64, reply: &mut ReplyDirectory) -> bool;
}

pub struct InodeGenerator {
    current: u64,
}

impl InodeGenerator {
    fn new() -> Self {
        InodeGenerator { current: 4 }
    }

    pub fn next(&mut self) -> u64 {
        let inode = self.current;
        self.current += 1;
        inode
    }
}

pub fn directory_attr(ino: u64) -> FileAttr {
    FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::Directory,
        perm: 0o755,
        nlink: 2,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

pub fn regular_file_attr(ino: u64, size: u64) -> FileAttr {
    FileAttr {
        ino,
        size,
        blocks: 1,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        flags: 0,
        blksize: 512,
    }
}

fn root_attr() -> FileAttr {
    directory_attr(ROOT_INO)
}

fn hello_attr() -> FileAttr {
    regular_file_attr(HELLO_INO, HELLO_CONTENT.len() as u64)
}

struct CybershuttleFS {
    data_sources: Vec<Box<dyn VirtualDataSource>>,
}

impl Filesystem for CybershuttleFS {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        for ds in &self.data_sources {
            if parent == ROOT_INO && name.to_str() == Some(ds.name()) {
                if let Some(attr) = ds.getattr(ds.inode()) {
                    reply.entry(&TTL, &attr, 0);
                    return;
                }
            }

            if let Some(attr) = ds.lookup(parent, name) {
                reply.entry(&TTL, &attr, 0);
                return;
            }
        }

        if parent == ROOT_INO && name.to_str() == Some("hello.txt") {
            reply.entry(&TTL, &hello_attr(), 0);
        } else {
            reply.error(ENOENT);
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyAttr) {
        match ino {
            ROOT_INO => reply.attr(&TTL, &root_attr()),
            HELLO_INO => reply.attr(&TTL, &hello_attr()),
            _ => {
                for ds in &self.data_sources {
                    if let Some(attr) = ds.getattr(ino) {
                        reply.attr(&TTL, &attr);
                        return;
                    }
                }
                reply.error(ENOENT);
            }
        }
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        if ino == HELLO_INO {
            let data = HELLO_CONTENT.as_bytes();
            let start = (offset as usize).min(data.len());
            let end = (start + size as usize).min(data.len());
            reply.data(&data[start..end]);
            return;
        }

        for ds in &self.data_sources {
            if let Some(data) = ds.read(ino, offset, size) {
                reply.data(&data);
                return;
            }
        }

        reply.error(ENOENT);
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino == ROOT_INO {
            let mut entries = vec![
                (ROOT_INO, FileType::Directory, "."),
                (ROOT_INO, FileType::Directory, ".."),
            ];

            for ds in &self.data_sources {
                entries.push((ds.inode(), FileType::Directory, ds.name()));
            }

            for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
                if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                    break;
                }
            }
            reply.ok();
            return;
        }

        for ds in &self.data_sources {
            if ds.readdir(ino, offset, &mut reply) {
                reply.ok();
                return;
            }
        }

        reply.error(ENOENT);
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: cs-filesystem <fuse|materialize|nfs> <tsv_path> <output_path|bind_addr>");
        std::process::exit(1);
    }

    let mode = &args[1];
    let tsv_path = &args[2];
    let output_path = &args[3];

    let mut inode_gen = InodeGenerator::new();
    let atlas_ds = atlas::load_atlas_datasource(tsv_path, &mut inode_gen);
    println!("Loaded {} ATLAS entries", atlas_ds.entry_count());

    if mode == "nfs" {
        if let Err(e) = nfs_server::serve(atlas_ds, output_path).await {
            eprintln!("Failed to serve NFS filesystem: {e}");
            std::process::exit(1);
        }
        return;
    }

    if mode == "materialize" {
        if let Err(e) = atlas_ds.materialize(output_path) {
            eprintln!("Failed to materialize ATLAS filesystem: {e}");
            std::process::exit(1);
        }

        println!("Materialized ATLAS filesystem at {output_path}");
        return;
    }

    if mode != "fuse" {
        eprintln!("Unknown mode: {mode}");
        eprintln!("Usage: cs-filesystem <fuse|materialize|nfs> <tsv_path> <output_path|bind_addr>");
        std::process::exit(1);
    }

    let fs = CybershuttleFS {
        data_sources: vec![Box::new(atlas_ds)],
    };

    let options = vec![
        MountOption::RO,
        MountOption::FSName("cybershuttlefs".to_string()),
        MountOption::AutoUnmount,
    ];

    if let Err(e) = fuser::mount2(fs, output_path, &options) {
        eprintln!("Failed to mount filesystem: {e}");
        std::process::exit(1);
    }
}
