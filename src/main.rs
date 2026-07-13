// Read-only CS filesystem for exposing Cybershuttle data sources.
//
// The FUSE mode mounts pluggable data sources as a virtual filesystem. The
// materialize mode writes the same logical tree to disk for NFS export.

mod atlas;
mod dataset_registry;
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
const REGISTRY_INO: u64 = 4;
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
        InodeGenerator { current: 5 }
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

fn registry_attr(registry_json: &str) -> FileAttr {
    regular_file_attr(REGISTRY_INO, registry_json.len() as u64)
}

struct CybershuttleFS {
    data_sources: Vec<Box<dyn VirtualDataSource>>,
    index_json: String,
    registry_json: String,
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

fn materialize_registry_json(output_path: &str) -> std::io::Result<()> {
    fs::create_dir_all(output_path)?;
    fs::write(
        Path::new(output_path).join(dataset_registry::REGISTRY_FILE_NAME),
        dataset_registry::official_dataset_registry_json(),
    )
}

fn usage() -> &'static str {
    "Usage: cs-filesystem <fuse|materialize|nfs> <atlas_tsv> [dataset=table_path ...] <output_path|bind_addr>"
}

fn parse_table_specs(args: &[String]) -> (Vec<(String, String)>, &str) {
    let output_path = args.last().expect("argument length checked").as_str();
    let table_args = &args[3..args.len() - 1];

    if table_args.iter().all(|arg| arg.contains('=')) {
        let specs = table_args
            .iter()
            .map(|arg| {
                let (name, path) = arg
                    .split_once('=')
                    .expect("contains check guarantees a split");
                if name.is_empty() || path.is_empty() {
                    eprintln!("Invalid dataset table spec: {arg}");
                    eprintln!("{}", usage());
                    std::process::exit(1);
                }
                (name.to_string(), path.to_string())
            })
            .collect();
        return (specs, output_path);
    }

    let legacy_names = ["mdcath", "memprotmd", "gpcrmd"];
    if table_args.len() > legacy_names.len() {
        eprintln!("{}", usage());
        std::process::exit(1);
    }

    let specs = table_args
        .iter()
        .zip(legacy_names)
        .map(|(path, name)| (name.to_string(), path.to_string()))
        .collect();

    (specs, output_path)
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

        if parent == ROOT_INO && name.to_str() == Some(dataset_registry::REGISTRY_FILE_NAME) {
            reply.entry(&TTL, &registry_attr(&self.registry_json), 0);
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
            REGISTRY_INO => reply.attr(&TTL, &registry_attr(&self.registry_json)),
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

        if ino == REGISTRY_INO {
            let data = self.registry_json.as_bytes();
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
                (
                    REGISTRY_INO,
                    FileType::RegularFile,
                    dataset_registry::REGISTRY_FILE_NAME,
                ),
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
        eprintln!("{}", usage());
        std::process::exit(1);
    }

    let mode = &args[1];
    let atlas_tsv_path = &args[2];
    let (table_specs, output_path) = parse_table_specs(&args);

    let mut inode_gen = InodeGenerator::new();
    let official_datasets_ds = table_dataset::TableDataSource::new(
        "datasets",
        dataset_registry::official_dataset_table_entries(),
        &mut inode_gen,
    );
    println!(
        "Loaded {} official dataset registry entries",
        official_datasets_ds.entry_count()
    );

    let atlas_ds = atlas::load_atlas_datasource(atlas_tsv_path, &mut inode_gen);
    println!("Loaded {} ATLAS entries", atlas_ds.entry_count());

    let table_data_sources: Vec<table_dataset::TableDataSource> = table_specs
        .iter()
        .map(|(name, path)| {
            let ds = table_dataset::load_table_datasource(name, path, &mut inode_gen);
            println!("Loaded {} {name} entries", ds.entry_count());
            ds
        })
        .collect();

    if mode == "nfs" {
        let mut datasets = Vec::new();
        datasets.push(table_dataset_to_nfs_dataset(&official_datasets_ds));
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

        for ds in &table_data_sources {
            datasets.push(table_dataset_to_nfs_dataset(ds));
        }

        if let Err(e) = nfs_server::serve(
            datasets,
            dataset_registry::official_dataset_registry_json(),
            output_path,
        )
        .await
        {
            eprintln!("Failed to serve NFS filesystem: {e}");
            std::process::exit(1);
        }
        return;
    }

    if mode == "materialize" {
        let mut index_sources: Vec<&dyn VirtualDataSource> = vec![&official_datasets_ds, &atlas_ds];
        for ds in &table_data_sources {
            index_sources.push(ds);
        }

        if let Err(e) = materialize_index_json(output_path, &index_sources) {
            eprintln!("Failed to materialize index.json: {e}");
            std::process::exit(1);
        }

        if let Err(e) = materialize_registry_json(output_path) {
            eprintln!("Failed to materialize registry.json: {e}");
            std::process::exit(1);
        }

        if let Err(e) = atlas_ds.materialize(output_path) {
            eprintln!("Failed to materialize ATLAS filesystem: {e}");
            std::process::exit(1);
        }

        if let Err(e) = official_datasets_ds.materialize(output_path) {
            eprintln!("Failed to materialize official dataset registry filesystem: {e}");
            std::process::exit(1);
        }

        for ds in &table_data_sources {
            if let Err(e) = ds.materialize(output_path) {
                eprintln!("Failed to materialize {} filesystem: {e}", ds.name());
                std::process::exit(1);
            }
        }

        println!("Materialized filesystem at {output_path}");
        return;
    }

    if mode != "fuse" {
        eprintln!("Unknown mode: {mode}");
        eprintln!("{}", usage());
        std::process::exit(1);
    }

    let mut data_sources: Vec<Box<dyn VirtualDataSource>> =
        vec![Box::new(official_datasets_ds), Box::new(atlas_ds)];
    for ds in table_data_sources {
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
        registry_json: dataset_registry::official_dataset_registry_json(),
    };

    let options = vec![
        MountOption::RO,
        MountOption::FSName("cybershuttlefs".to_string()),
    ];

    if let Err(e) = fuser::mount2(fs, output_path, &options) {
        eprintln!("Failed to mount filesystem: {e}");
        std::process::exit(1);
    }
}
