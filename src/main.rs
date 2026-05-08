// Simple in-memory FUSE filesystem using the `fuser` crate.
//
// Exposes a single read-only directory containing one file:
//   /hello.txt  -> "Hello, FUSE from Rust!\n"
//
// Usage:
//   mkdir /tmp/myfs
//   cargo run --release -- /tmp/myfs
//   cat /tmp/myfs/hello.txt
//   fusermount -u /tmp/myfs   (Ctrl-C also unmounts)

use std::ffi::OsStr;
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use libc::ENOENT;


struct DataSource {
    inode_no: u64,
    name: String,
}


impl DataSource {

    fn new(inode_no: u64, name: String) -> Self {
        DataSource { inode_no, name }
    }

    fn get_attr(&self, ) -> FileAttr {
        FileAttr {
            ino: self.inode_no,
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
}


// implement an incrementing inode number generator
struct InodeGenerator {
    current: u64,
}   

impl InodeGenerator {
    fn new() -> Self {
        InodeGenerator { current: 4 } // Start from 4 since 1, 2, and 3 are already used
    }

    fn next(&mut self) -> u64 {
        let inode = self.current;
        self.current += 1;
        inode
    }
}


const TTL: Duration = Duration::from_secs(1);

const HELLO_CONTENT: &str = "Hello, FUSE from Rust!\n";

const ROOT_INO: u64 = 1;
const HELLO_INO: u64 = 2;

fn root_attr() -> FileAttr {
    FileAttr {
        ino: ROOT_INO,
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

fn hello_attr() -> FileAttr {
    FileAttr {
        ino: HELLO_INO,
        size: HELLO_CONTENT.len() as u64,
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
struct CybershuttleFS {
    data_sources: Vec<DataSource>, 
}

impl Filesystem for CybershuttleFS {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {

        for ds in &self.data_sources {
            if parent == ROOT_INO && name.to_str() == Some(ds.name.as_str()) {
                reply.entry(&TTL, &ds.get_attr(), 0);
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
                    if ino == ds.inode_no {
                        reply.attr(&TTL, &ds.get_attr());
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
        if ino != HELLO_INO {
            reply.error(ENOENT);
            return;
        }
        let data = HELLO_CONTENT.as_bytes();
        let start = (offset as usize).min(data.len());
        let end = (start + size as usize).min(data.len());
        reply.data(&data[start..end]);
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino != ROOT_INO {
            reply.error(ENOENT);
            return;
        }

        let mut entries = vec![
            (ROOT_INO, FileType::Directory, "."),
            (ROOT_INO, FileType::Directory, ".."),
            (HELLO_INO, FileType::RegularFile, "hello.txt"),
        ];
        
        for ds in self.data_sources.iter() {
            entries.push((ds.inode_no, FileType::Directory, ds.name.as_str()));
        }

        for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
            // i + 1 is the next offset to resume from.
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                break;
            }
        }
        reply.ok();
    }
}

fn main() {
    env_logger::init();

    let mountpoint = std::env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cs-filesystem <MOUNTPOINT>");
        std::process::exit(1);
    });

    let options = vec![
        MountOption::RO,
        MountOption::FSName("cybershuttlefs".to_string()),
        MountOption::AutoUnmount,
        MountOption::AllowOther,
    ];


    let mut inode_gen = InodeGenerator::new();

    let data_sources = vec![
        DataSource::new(inode_gen.next(), "alphafold".to_string()),
        DataSource::new(inode_gen.next(), "protein_data".to_string()),
    ];


    let fs = CybershuttleFS { data_sources };

    if let Err(e) = fuser::mount2(fs, &mountpoint, &options) {
        eprintln!("Failed to mount filesystem: {e}");
        std::process::exit(1);
    }
}
