// Read-only CS filesystem for exposing Cybershuttle data sources.
//
// The FUSE mode mounts pluggable data sources as a virtual filesystem. The
// materialize mode writes the same logical tree to disk for NFS export.

mod atlas;
mod nfs_server;
mod table_dataset;

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry,
    Request,
};
use libc::ENOENT;

const TTL: Duration = Duration::from_secs(1);
const ROOT_INO: u64 = 1;
const HELLO_INO: u64 = 2;
const INDEX_INO: u64 = 3;
const HELLO_CONTENT: &str = "Hello, FUSE from Rust!\n";
const INDEX_FILE_NAME: &str = "index.json";

pub trait VirtualDataSource {
    fn name(&self) -> &str;
    fn kind(&self) -> &str;
    fn inode(&self) -> u64;
    fn entry_count(&self) -> usize;

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

fn index_attr(index_json: &str) -> FileAttr {
    regular_file_attr(INDEX_INO, index_json.len() as u64)
}

struct CybershuttleFS {
    data_sources: Vec<Box<dyn VirtualDataSource>>,
    index_json: String,
}

fn build_index_json(data_sources: &[&dyn VirtualDataSource]) -> String {
    let datasets: Vec<serde_json::Value> = data_sources
        .iter()
        .map(|ds| {
            serde_json::json!({
                "name": ds.name(),
                "kind": ds.kind(),
                "entries": ds.entry_count(),
            })
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({ "datasets": datasets })).unwrap()
}

fn materialize_index_json(
    output_path: &str,
    data_sources: &[&dyn VirtualDataSource],
) -> std::io::Result<()> {
    fs::create_dir_all(output_path)?;
    fs::write(
        Path::new(output_path).join(INDEX_FILE_NAME),
        build_index_json(data_sources),
    )
}

fn table_dataset_to_nfs_dataset(ds: &table_dataset::TableDataSource) -> nfs_server::NfsDataset {
    nfs_server::NfsDataset {
        name: ds.name().to_string(),
        kind: ds.kind().to_string(),
        entries: ds
            .entry_ids()
            .into_iter()
            .map(|id| nfs_server::NfsEntry {
                metadata_json: ds.metadata_json(&id).unwrap_or("").to_string(),
                id,
            })
            .collect(),
    }
}

impl Filesystem for CybershuttleFS {
    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if parent == ROOT_INO && name.to_str() == Some(INDEX_FILE_NAME) {
            reply.entry(&TTL, &index_attr(&self.index_json), 0);
            return;
        }

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
            INDEX_INO => reply.attr(&TTL, &index_attr(&self.index_json)),
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

        if ino == INDEX_INO {
            let data = self.index_json.as_bytes();
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
                (INDEX_INO, FileType::RegularFile, INDEX_FILE_NAME),
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
    if args.len() < 4 || args.len() > 7 {
        eprintln!(
            "Usage: cs-filesystem <fuse|materialize|nfs> <atlas_tsv> [mdcath_table] [memprotmd_table] [gpcrmd_table] <output_path|bind_addr>"
        );
        std::process::exit(1);
    }

    let mode = &args[1];
    let atlas_tsv_path = &args[2];
    let (mdcath_table_path, memprotmd_table_path, gpcrmd_table_path, output_path) = match args.len()
    {
        4 => (None, None, None, args[3].as_str()),
        5 => (Some(args[3].as_str()), None, None, args[4].as_str()),
        6 => (
            Some(args[3].as_str()),
            Some(args[4].as_str()),
            None,
            args[5].as_str(),
        ),
        _ => (
            Some(args[3].as_str()),
            Some(args[4].as_str()),
            Some(args[5].as_str()),
            args[6].as_str(),
        ),
    };

    let mut inode_gen = InodeGenerator::new();
    let atlas_ds = atlas::load_atlas_datasource(atlas_tsv_path, &mut inode_gen);
    println!("Loaded {} ATLAS entries", atlas_ds.entry_count());

    let mdcath_ds = mdcath_table_path.map(|path| {
        let ds = table_dataset::load_table_datasource("mdcath", path, &mut inode_gen);
        println!("Loaded {} mdCATH entries", ds.entry_count());
        ds
    });

    let memprotmd_ds = memprotmd_table_path.map(|path| {
        let ds = table_dataset::load_table_datasource("memprotmd", path, &mut inode_gen);
        println!("Loaded {} MemProtMD entries", ds.entry_count());
        ds
    });

    let gpcrmd_ds = gpcrmd_table_path.map(|path| {
        let ds = table_dataset::load_table_datasource("gpcrmd", path, &mut inode_gen);
        println!("Loaded {} GPCRmd entries", ds.entry_count());
        ds
    });

    if mode == "nfs" {
        let mut datasets = Vec::new();
        datasets.push(nfs_server::NfsDataset {
            name: "atlas".to_string(),
            kind: atlas_ds.kind().to_string(),
            entries: atlas_ds
                .entry_ids()
                .into_iter()
                .map(|id| nfs_server::NfsEntry {
                    metadata_json: atlas_ds.metadata_json(&id).unwrap_or("").to_string(),
                    id,
                })
                .collect(),
        });

        if let Some(ds) = &mdcath_ds {
            datasets.push(table_dataset_to_nfs_dataset(ds));
        }

        if let Some(ds) = &memprotmd_ds {
            datasets.push(table_dataset_to_nfs_dataset(ds));
        }

        if let Some(ds) = &gpcrmd_ds {
            datasets.push(table_dataset_to_nfs_dataset(ds));
        }

        if let Err(e) = nfs_server::serve(datasets, output_path).await {
            eprintln!("Failed to serve NFS filesystem: {e}");
            std::process::exit(1);
        }
        return;
    }

    if mode == "materialize" {
        let mut index_sources: Vec<&dyn VirtualDataSource> = vec![&atlas_ds];
        if let Some(ds) = &mdcath_ds {
            index_sources.push(ds);
        }
        if let Some(ds) = &memprotmd_ds {
            index_sources.push(ds);
        }
        if let Some(ds) = &gpcrmd_ds {
            index_sources.push(ds);
        }

        if let Err(e) = materialize_index_json(output_path, &index_sources) {
            eprintln!("Failed to materialize index.json: {e}");
            std::process::exit(1);
        }

        if let Err(e) = atlas_ds.materialize(output_path) {
            eprintln!("Failed to materialize ATLAS filesystem: {e}");
            std::process::exit(1);
        }

        if let Some(ds) = &mdcath_ds {
            if let Err(e) = ds.materialize(output_path) {
                eprintln!("Failed to materialize mdCATH filesystem: {e}");
                std::process::exit(1);
            }
        }

        if let Some(ds) = &memprotmd_ds {
            if let Err(e) = ds.materialize(output_path) {
                eprintln!("Failed to materialize MemProtMD filesystem: {e}");
                std::process::exit(1);
            }
        }

        if let Some(ds) = &gpcrmd_ds {
            if let Err(e) = ds.materialize(output_path) {
                eprintln!("Failed to materialize GPCRmd filesystem: {e}");
                std::process::exit(1);
            }
        }

        println!("Materialized filesystem at {output_path}");
        return;
    }

    if mode != "fuse" {
        eprintln!("Unknown mode: {mode}");
        eprintln!(
            "Usage: cs-filesystem <fuse|materialize|nfs> <atlas_tsv> [mdcath_table] [memprotmd_table] [gpcrmd_table] <output_path|bind_addr>"
        );
        std::process::exit(1);
    }

    let mut data_sources: Vec<Box<dyn VirtualDataSource>> = vec![Box::new(atlas_ds)];
    if let Some(ds) = mdcath_ds {
        data_sources.push(Box::new(ds));
    }
    if let Some(ds) = memprotmd_ds {
        data_sources.push(Box::new(ds));
    }
    if let Some(ds) = gpcrmd_ds {
        data_sources.push(Box::new(ds));
    }

    let index_sources: Vec<&dyn VirtualDataSource> = data_sources
        .iter()
        .map(|ds| ds.as_ref() as &dyn VirtualDataSource)
        .collect();
    let index_json = build_index_json(&index_sources);

    let fs = CybershuttleFS {
        data_sources,
        index_json,
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
