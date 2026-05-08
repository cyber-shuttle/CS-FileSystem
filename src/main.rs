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

struct CSFile {
    inode_no: u64,
    cs_data_id: String,
    name: String,
}

struct CSDirectory {
    inode_no: u64,
    name: String,
    cs_data_id: String,
    files: Vec<CSFile>,
    directories: Vec<CSDirectory>,
}

struct DataSource {
    inode_no: u64,
    name: String,
    directories: Vec<CSDirectory>,
    files: Vec<CSFile>,
}

fn lookup_inode_in_directory(dir: &CSDirectory, inode_no: u64) -> Option<FileAttr> {
    if inode_no == dir.inode_no {
        return Some(FileAttr {
            ino: dir.inode_no,
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
        });
    }

    for file in &dir.files {
        if inode_no == file.inode_no {
            return Some(FileAttr {
                ino: file.inode_no,
                size: 0,
                blocks: 0,
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
            });
        }
    }

    for subdir in &dir.directories {
        if let Some(attr) = lookup_inode_in_directory(subdir, inode_no) {
            return Some(attr);
        }
    }

    None
}

fn lookup_attr_in_directory(dir: &CSDirectory, name: &OsStr, parent_inode: u64) -> Option<FileAttr> {
    
    for file in &dir.files {
        if parent_inode == dir.inode_no && name.to_str() == Some(file.name.as_str()) {
            return Some(FileAttr {
                ino: file.inode_no,
                size: 0,
                blocks: 0,
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
            });
        }
    }

    for subdir in &dir.directories {
        if parent_inode == dir.inode_no && name.to_str() == Some(subdir.name.as_str()) {
            return Some(FileAttr {
                ino: subdir.inode_no,
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
            });
        }
    }

    for subdir in &dir.directories {
        if let Some(attr) = lookup_attr_in_directory(subdir, name, parent_inode) {
            return Some(attr);
        }
    }


    None
}


impl DataSource {

    fn new(inode_no: u64, name: String, directories: Vec<CSDirectory>, files: Vec<CSFile>) -> Self {
        DataSource { inode_no, name, directories, files }
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

    fn lookup_by_inode(&self, inode_no: u64) -> Option<FileAttr> {
        if inode_no == self.inode_no {
            return Some(self.get_attr());
        }

        for file in &self.files {
            if inode_no == file.inode_no {
                return Some(FileAttr {
                    ino: file.inode_no,
                    size: 0,
                    blocks: 0,
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
                });
            }
        }

        for dir in &self.directories {
            if let Some(attr) = lookup_inode_in_directory(dir, inode_no) {
                return Some(attr);
            }
        }
        None
    }


    fn lookup_by_name(&self, parent_inode: u64, name: &OsStr) -> Option<FileAttr> {
        for file in &self.files {
            if parent_inode == self.inode_no && name.to_str() == Some(file.name.as_str()) {
                return Some(FileAttr {
                    ino: file.inode_no,
                    size: 0,
                    blocks: 0,
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
                });
            }
        }

        for dir in &self.directories {
            if let Some(attr) = lookup_attr_in_directory(dir, name, parent_inode) {
                return Some(attr);
            }
        }
        None
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
            if let Some(attr) = ds.lookup_by_name(parent, name) {
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
                    if let Some(attr) = ds.lookup_by_inode(ino) {
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

        if ino == ROOT_INO {
            // Add the "hello.txt" entry to the root directory
            let mut entries = vec![
                (ROOT_INO, FileType::Directory, "."),
                (ROOT_INO, FileType::Directory, ".."),
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
            return;
        } else {

            for ds in &self.data_sources {
                if let Some(attr) = ds.lookup_by_inode(ino) {
                    if attr.kind == FileType::Directory {
                        let mut entries = vec![
                            (attr.ino, FileType::Directory, "."),
                            (ROOT_INO, FileType::Directory, ".."),
                        ];

                        for file in &ds.files {
                            entries.push((file.inode_no, FileType::RegularFile, file.name.as_str()));
                        }

                        for dir in &ds.directories {
                            entries.push((dir.inode_no, FileType::Directory, dir.name.as_str()));
                        }

                        for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
                            // i + 1 is the next offset to resume from.
                            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2) {
                                break;
                            }
                        }
                        reply.ok();
                        return;
                    }
                }
            }
        }
        reply.error(ENOENT);
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

    let alp_dirs = vec![
        CSDirectory {
            inode_no: inode_gen.next(),
            name: "pdb".to_string(),
            cs_data_id: "alphafold_pdb".to_string(),
            files: vec![],
            directories: vec![],
        },
        CSDirectory {
            inode_no: inode_gen.next(),
            name: "fasta".to_string(),
            cs_data_id: "alphafold_fasta".to_string(),
            files: vec![],
            directories: vec![],
        },
    ];


    let alp_files = vec![
        CSFile {
            inode_no: inode_gen.next(),
            cs_data_id: "alphafold_summary".to_string(),
            name: "summary.txt".to_string(),
        },
    ];


    let protein_data_dirs = vec![
        CSDirectory {
            inode_no: inode_gen.next(),
            name: "uniprot".to_string(),
            cs_data_id: "protein_data_uniprot".to_string(),
            files: vec![],
            directories: vec![],
        },
    ];


    let data_sources = vec![
        DataSource::new(inode_gen.next(), "alphafold".to_string(), alp_dirs, alp_files),
        DataSource::new(inode_gen.next(), "protein_data".to_string(), protein_data_dirs, vec![]),
    ];


    let fs = CybershuttleFS { data_sources };

    if let Err(e) = fuser::mount2(fs, &mountpoint, &options) {
        eprintln!("Failed to mount filesystem: {e}");
        std::process::exit(1);
    }
}
