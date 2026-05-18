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

mod atlas;
use atlas::{load_atlas, AtlasEntry};
use std::collections::HashMap;
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

fn directory_attr(ino: u64) -> FileAttr {
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

fn regular_file_attr(ino: u64, size: u64) -> FileAttr {
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

fn build_atlas_datasource(
    entries: &[AtlasEntry],
    inode_gen: &mut InodeGenerator,
    file_contents: &mut HashMap<u64, String>,
) -> DataSource {
    let mut directories = Vec::new();

    for entry in entries {
        let file_inode = inode_gen.next();

        file_contents.insert(file_inode, entry.metadata_json.clone());

        let metadata_file = CSFile {
            inode_no: file_inode,
            cs_data_id: format!("{}_metadata", entry.id),
            name: "metadata.json".to_string(),
        };

        let dir = CSDirectory {
            inode_no: inode_gen.next(),
            name: entry.id.clone(),
            cs_data_id: entry.id.clone(),
            files: vec![metadata_file],
            directories: vec![],
        };
        directories.push(dir);
    }

    DataSource::new(inode_gen.next(), "atlas".to_string(), directories, vec![])
}

fn lookup_inode_in_directory(
    dir: &CSDirectory,
    inode_no: u64,
    file_contents: &HashMap<u64, String>,
) -> Option<FileAttr> {
    if inode_no == dir.inode_no {
        return Some(directory_attr(dir.inode_no));
    }

    for file in &dir.files {
        if inode_no == file.inode_no {
            let size = file_contents
                .get(&file.inode_no)
                .map_or(0, |content| content.len() as u64);
            return Some(regular_file_attr(file.inode_no, size));
        }
    }

    for subdir in &dir.directories {
        if let Some(attr) = lookup_inode_in_directory(subdir, inode_no, file_contents) {
            return Some(attr);
        }
    }

    None
}

fn lookup_attr_in_directory(
    dir: &CSDirectory,
    name: &OsStr,
    parent_inode: u64,
    file_contents: &HashMap<u64, String>,
) -> Option<FileAttr> {
    for file in &dir.files {
        if parent_inode == dir.inode_no && name.to_str() == Some(file.name.as_str()) {
            let size = file_contents
                .get(&file.inode_no)
                .map_or(0, |content| content.len() as u64);
            return Some(regular_file_attr(file.inode_no, size));
        }
    }

    for subdir in &dir.directories {
        if parent_inode == dir.inode_no && name.to_str() == Some(subdir.name.as_str()) {
            return Some(directory_attr(subdir.inode_no));
        }
    }

    for subdir in &dir.directories {
        if let Some(attr) = lookup_attr_in_directory(subdir, name, parent_inode, file_contents) {
            return Some(attr);
        }
    }

    None
}

fn find_directory_listing<'a>(
    dir: &'a CSDirectory,
    inode_no: u64,
    parent_inode: u64,
) -> Option<(u64, &'a [CSFile], &'a [CSDirectory])> {
    if inode_no == dir.inode_no {
        return Some((parent_inode, &dir.files, &dir.directories));
    }

    for subdir in &dir.directories {
        if let Some(listing) = find_directory_listing(subdir, inode_no, dir.inode_no) {
            return Some(listing);
        }
    }

    None
}

impl DataSource {
    fn new(inode_no: u64, name: String, directories: Vec<CSDirectory>, files: Vec<CSFile>) -> Self {
        DataSource {
            inode_no,
            name,
            directories,
            files,
        }
    }

    fn get_attr(&self) -> FileAttr {
        directory_attr(self.inode_no)
    }

    fn lookup_by_inode(
        &self,
        inode_no: u64,
        file_contents: &HashMap<u64, String>,
    ) -> Option<FileAttr> {
        if inode_no == self.inode_no {
            return Some(self.get_attr());
        }

        for file in &self.files {
            if inode_no == file.inode_no {
                let size = file_contents
                    .get(&file.inode_no)
                    .map_or(0, |content| content.len() as u64);
                return Some(regular_file_attr(file.inode_no, size));
            }
        }

        for dir in &self.directories {
            if let Some(attr) = lookup_inode_in_directory(dir, inode_no, file_contents) {
                return Some(attr);
            }
        }
        None
    }

    fn lookup_by_name(
        &self,
        parent_inode: u64,
        name: &OsStr,
        file_contents: &HashMap<u64, String>,
    ) -> Option<FileAttr> {
        for file in &self.files {
            if parent_inode == self.inode_no && name.to_str() == Some(file.name.as_str()) {
                let size = file_contents
                    .get(&file.inode_no)
                    .map_or(0, |content| content.len() as u64);
                return Some(regular_file_attr(file.inode_no, size));
            }
        }

        for dir in &self.directories {
            if parent_inode == self.inode_no && name.to_str() == Some(dir.name.as_str()) {
                return Some(directory_attr(dir.inode_no));
            }
        }

        for dir in &self.directories {
            if let Some(attr) = lookup_attr_in_directory(dir, name, parent_inode, file_contents) {
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
    file_contents: HashMap<u64, String>, //inode -> content
}

impl Filesystem for CybershuttleFS {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        for ds in &self.data_sources {
            if parent == ROOT_INO && name.to_str() == Some(ds.name.as_str()) {
                reply.entry(&TTL, &ds.get_attr(), 0);
                return;
            }
            if let Some(attr) = ds.lookup_by_name(parent, name, &self.file_contents) {
                if let Some(content) = self.file_contents.get(&attr.ino) {
                    let file_attr = regular_file_attr(attr.ino, content.len() as u64);
                    reply.entry(&TTL, &file_attr, 0);
                } else {
                    reply.entry(&TTL, &attr, 0);
                }
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
        if let Some(content) = self.file_contents.get(&ino) {
            reply.attr(&TTL, &regular_file_attr(ino, content.len() as u64));
            return;
        }

        match ino {
            ROOT_INO => reply.attr(&TTL, &root_attr()),
            HELLO_INO => reply.attr(&TTL, &hello_attr()),
            _ => {
                for ds in &self.data_sources {
                    if let Some(attr) = ds.lookup_by_inode(ino, &self.file_contents) {
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
        if let Some(content) = self.file_contents.get(&ino) {
            let data = content.as_bytes();
            let start = (offset as usize).min(data.len());
            let end = (start + size as usize).min(data.len());
            reply.data(&data[start..end]);
        } else if ino == HELLO_INO {
            let data = HELLO_CONTENT.as_bytes();
            let start = (offset as usize).min(data.len());
            let end = (start + size as usize).min(data.len());
            reply.data(&data[start..end]);
        } else {
            reply.error(ENOENT);
        }
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
                let listing = if ino == ds.inode_no {
                    Some((ROOT_INO, ds.files.as_slice(), ds.directories.as_slice()))
                } else {
                    ds.directories
                        .iter()
                        .find_map(|dir| find_directory_listing(dir, ino, ds.inode_no))
                };

                if let Some((parent_inode, files, directories)) = listing {
                    let mut entries = vec![
                        (ino, FileType::Directory, "."),
                        (parent_inode, FileType::Directory, ".."),
                    ];

                    for file in files {
                        entries.push((file.inode_no, FileType::RegularFile, file.name.as_str()));
                    }

                    for dir in directories {
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
        reply.error(ENOENT);
    }
}

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: cs-filesystem <tsv_path> <mountpoint>");
        std::process::exit(1);
    }

    let tsv_path = &args[1];
    let mountpoint = &args[2];

    let entries = load_atlas(tsv_path);
    println!("Loaded {} ATLAS entries", entries.len());

    let mut inode_gen = InodeGenerator::new();
    let mut file_contents = HashMap::new();
    let atlas_ds = build_atlas_datasource(&entries, &mut inode_gen, &mut file_contents);

    let data_sources = vec![atlas_ds];
    let fs = CybershuttleFS {
        data_sources,
        file_contents,
    };

    let options = vec![
        MountOption::RO,
        MountOption::FSName("cybershuttlefs".to_string()),
        MountOption::AutoUnmount,
    ];

    if let Err(e) = fuser::mount2(fs, mountpoint, &options) {
        eprintln!("Failed to mount filesystem: {e}");
        std::process::exit(1);
    }
}
