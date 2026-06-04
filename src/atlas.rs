use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};

use fuser::{FileAttr, FileType, ReplyDirectory};
use std::path::Path;

use crate::{directory_attr, regular_file_attr, InodeGenerator, VirtualDataSource};

const METADATA_FILE_NAME: &str = "metadata.json";

#[derive(Clone, Debug)]
pub struct AtlasEntry {
    pub id: String,
    pub metadata_json: String,
}

#[derive(Clone, Debug)]
struct AtlasEntryNode {
    metadata_inode: u64,
    id: String,
}

pub struct AtlasDataSource {
    inode: u64,
    entry_dirs: HashMap<u64, AtlasEntryNode>,
    entry_name_to_inode: HashMap<String, u64>,
    file_contents: HashMap<u64, String>,
}

impl AtlasDataSource {
    pub fn new(entries: Vec<AtlasEntry>, inode_gen: &mut InodeGenerator) -> Self {
        let inode = inode_gen.next();
        let mut entry_dirs = HashMap::new();
        let mut entry_name_to_inode = HashMap::new();
        let mut file_contents = HashMap::new();

        for entry in entries {
            let entry_inode = inode_gen.next();
            let metadata_inode = inode_gen.next();

            entry_name_to_inode.insert(entry.id.clone(), entry_inode);
            file_contents.insert(metadata_inode, entry.metadata_json);

            entry_dirs.insert(
                entry_inode,
                AtlasEntryNode {
                    metadata_inode,
                    id: entry.id,
                },
            );
        }

        AtlasDataSource {
            inode,
            entry_dirs,
            entry_name_to_inode,
            file_contents,
        }
    }

    pub fn entry_count(&self) -> usize {
        self.entry_dirs.len()
    }
    pub fn entry_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .entry_dirs
            .values()
            .map(|node| node.id.clone())
            .collect();
        ids.sort();
        ids
    }
    pub fn metadata_json(&self, id: &str) -> Option<&str> {
        let entry_inode = self.entry_name_to_inode.get(id)?;
        let node = self.entry_dirs.get(entry_inode)?;
        self.file_contents
            .get(&node.metadata_inode)
            .map(String::as_str)
    }
    pub fn materialize(&self, output_dir: &str) -> std::io::Result<()> {
        let atlas_dir = Path::new(output_dir).join("atlas");
        fs::create_dir_all(&atlas_dir)?;

        for id in self.entry_ids() {
            let entry_dir = atlas_dir.join(&id);
            fs::create_dir_all(&entry_dir)?;

            if let Some(metadata_json) = self.metadata_json(&id) {
                fs::write(entry_dir.join(METADATA_FILE_NAME), metadata_json)?;
            }
        }
        Ok(())
    }
    fn metadata_attr(&self, metadata_inode: u64) -> Option<FileAttr> {
        self.file_contents
            .get(&metadata_inode)
            .map(|content| regular_file_attr(metadata_inode, content.len() as u64))
    }

    fn read_metadata(&self, inode: u64, offset: i64, size: u32) -> Option<Vec<u8>> {
        let content = self.file_contents.get(&inode)?;
        let data = content.as_bytes();
        let start = (offset as usize).min(data.len());
        let end = (start + size as usize).min(data.len());
        Some(data[start..end].to_vec())
    }

    fn entries_for_readdir(&self, ino: u64) -> Option<Vec<(u64, FileType, String)>> {
        if ino == self.inode {
            let mut entries = vec![
                (self.inode, FileType::Directory, ".".to_string()),
                (self.inode, FileType::Directory, "..".to_string()),
            ];

            let mut entry_dirs: Vec<_> = self.entry_dirs.iter().collect();
            entry_dirs.sort_by(|(_, left), (_, right)| left.id.cmp(&right.id));

            for (entry_inode, node) in entry_dirs {
                entries.push((*entry_inode, FileType::Directory, node.id.clone()));
            }

            return Some(entries);
        }

        let node = self.entry_dirs.get(&ino)?;
        Some(vec![
            (ino, FileType::Directory, ".".to_string()),
            (self.inode, FileType::Directory, "..".to_string()),
            (
                node.metadata_inode,
                FileType::RegularFile,
                METADATA_FILE_NAME.to_string(),
            ),
        ])
    }
}

pub fn parse_atlas_tsv(tsv_path: &str) -> Vec<AtlasEntry> {
    let file = fs::File::open(tsv_path).expect("Failed to open TSV");
    parse_atlas_reader(BufReader::new(file))
}

fn parse_atlas_reader<R: BufRead>(reader: R) -> Vec<AtlasEntry> {
    let mut lines = reader.lines();
    let header_line = lines
        .next()
        .expect("ATLAS TSV is missing a header row")
        .expect("Failed to read ATLAS TSV header");
    let headers: Vec<String> = header_line.split('\t').map(str::to_string).collect();
    let mut entries = Vec::new();

    for line in lines {
        let line = line.expect("Failed to read ATLAS TSV row");
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split('\t').collect();
        let mut map = serde_json::Map::new();

        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = fields.get(i) {
                map.insert(
                    header.to_string(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }

        let id = fields.get(0).unwrap_or(&"unknown").to_string();
        let metadata_json = serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap();
        entries.push(AtlasEntry { id, metadata_json });
    }

    entries
}

pub fn load_atlas_datasource(tsv_path: &str, inode_gen: &mut InodeGenerator) -> AtlasDataSource {
    AtlasDataSource::new(parse_atlas_tsv(tsv_path), inode_gen)
}

impl VirtualDataSource for AtlasDataSource {
    fn name(&self) -> &str {
        "atlas"
    }

    fn inode(&self) -> u64 {
        self.inode
    }

    fn lookup(&self, parent: u64, name: &OsStr) -> Option<FileAttr> {
        if parent == self.inode {
            let entry_name = name.to_str()?;
            let entry_inode = self.entry_name_to_inode.get(entry_name)?;
            return Some(directory_attr(*entry_inode));
        }

        let node = self.entry_dirs.get(&parent)?;
        if name.to_str() == Some(METADATA_FILE_NAME) {
            return self.metadata_attr(node.metadata_inode);
        }

        None
    }

    fn getattr(&self, ino: u64) -> Option<FileAttr> {
        if ino == self.inode || self.entry_dirs.contains_key(&ino) {
            return Some(directory_attr(ino));
        }

        self.metadata_attr(ino)
    }

    fn read(&self, ino: u64, offset: i64, size: u32) -> Option<Vec<u8>> {
        self.read_metadata(ino, offset, size)
    }

    fn readdir(&self, ino: u64, offset: i64, reply: &mut ReplyDirectory) -> bool {
        let Some(entries) = self.entries_for_readdir(ino) else {
            return false;
        };

        for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
            if reply.add(entry.0, (i + 1) as i64, entry.1, entry.2.as_str()) {
                break;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use fuser::FileType;

    use super::*;

    fn sample_entries() -> Vec<AtlasEntry> {
        parse_atlas_reader(Cursor::new(
            "PDB\tlength\tprotein_name\n1r6w_A\t322\to-succinylbenzoate synthase\n2y44_A\t184\tAlanine-rich surface protein\n",
        ))
    }

    fn sample_datasource() -> AtlasDataSource {
        let mut inode_gen = InodeGenerator::new();
        AtlasDataSource::new(sample_entries(), &mut inode_gen)
    }

    #[test]
    fn parses_tsv_rows_into_pretty_json_entries() {
        let entries = sample_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "1r6w_A");

        let metadata: serde_json::Value = serde_json::from_str(&entries[0].metadata_json).unwrap();
        assert_eq!(metadata["PDB"], "1r6w_A");
        assert_eq!(metadata["length"], "322");
        assert_eq!(metadata["protein_name"], "o-succinylbenzoate synthase");
    }

    #[test]
    fn skips_blank_tsv_rows() {
        let entries = parse_atlas_reader(Cursor::new("PDB\tlength\n\n1r6w_A\t322\n\n"));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "1r6w_A");
    }

    #[test]
    fn allocates_predictable_inodes_for_atlas_tree() {
        let ds = sample_datasource();

        assert_eq!(ds.inode(), 4);
        assert_eq!(ds.entry_count(), 2);
        assert_eq!(ds.entry_name_to_inode["1r6w_A"], 5);
        assert_eq!(ds.entry_dirs[&5].metadata_inode, 6);
        assert_eq!(ds.entry_name_to_inode["2y44_A"], 7);
        assert_eq!(ds.entry_dirs[&7].metadata_inode, 8);
    }

    #[test]
    fn lookup_finds_entry_directory_under_atlas_root() {
        let ds = sample_datasource();
        let attr = ds.lookup(ds.inode(), OsStr::new("1r6w_A")).unwrap();

        assert_eq!(attr.ino, 5);
        assert_eq!(attr.kind, FileType::Directory);
        assert_eq!(attr.perm, 0o755);
    }

    #[test]
    fn lookup_finds_metadata_file_inside_entry_directory() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["1r6w_A"];
        let attr = ds
            .lookup(entry_inode, OsStr::new(METADATA_FILE_NAME))
            .unwrap();

        assert_eq!(attr.ino, ds.entry_dirs[&entry_inode].metadata_inode);
        assert_eq!(attr.kind, FileType::RegularFile);
        assert!(attr.size > 0);
    }

    #[test]
    fn lookup_rejects_unknown_entry_and_unknown_file() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["1r6w_A"];

        assert!(ds.lookup(ds.inode(), OsStr::new("missing")).is_none());
        assert!(ds
            .lookup(entry_inode, OsStr::new("not_metadata.json"))
            .is_none());
    }

    #[test]
    fn getattr_reports_atlas_entry_and_metadata_attrs() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["1r6w_A"];
        let metadata_inode = ds.entry_dirs[&entry_inode].metadata_inode;

        assert_eq!(ds.getattr(ds.inode()).unwrap().kind, FileType::Directory);
        assert_eq!(ds.getattr(entry_inode).unwrap().kind, FileType::Directory);

        let metadata_attr = ds.getattr(metadata_inode).unwrap();
        assert_eq!(metadata_attr.kind, FileType::RegularFile);
        assert_eq!(
            metadata_attr.size,
            ds.file_contents[&metadata_inode].len() as u64
        );
    }

    #[test]
    fn read_returns_sliced_metadata_bytes() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["1r6w_A"];
        let metadata_inode = ds.entry_dirs[&entry_inode].metadata_inode;
        let full_content = ds.file_contents[&metadata_inode].clone();

        assert_eq!(
            String::from_utf8(ds.read(metadata_inode, 0, 20).unwrap()).unwrap(),
            full_content[..20].to_string()
        );
        assert_eq!(
            String::from_utf8(ds.read(metadata_inode, 5, 10).unwrap()).unwrap(),
            full_content[5..15].to_string()
        );
        assert_eq!(
            ds.read(metadata_inode, 99_999, 10).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn read_rejects_directory_inodes_and_unknown_inodes() {
        let ds = sample_datasource();

        assert!(ds.read(ds.inode(), 0, 10).is_none());
        assert!(ds.read(999, 0, 10).is_none());
    }

    #[test]
    fn root_listing_contains_sorted_entry_directories() {
        let ds = sample_datasource();
        let entries = ds.entries_for_readdir(ds.inode()).unwrap();

        assert_eq!(
            entries[0],
            (ds.inode(), FileType::Directory, ".".to_string())
        );
        assert_eq!(
            entries[1],
            (ds.inode(), FileType::Directory, "..".to_string())
        );
        assert_eq!(entries[2], (5, FileType::Directory, "1r6w_A".to_string()));
        assert_eq!(entries[3], (7, FileType::Directory, "2y44_A".to_string()));
    }

    #[test]
    fn entry_listing_contains_metadata_json() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["1r6w_A"];
        let metadata_inode = ds.entry_dirs[&entry_inode].metadata_inode;
        let entries = ds.entries_for_readdir(entry_inode).unwrap();

        assert_eq!(
            entries[0],
            (entry_inode, FileType::Directory, ".".to_string())
        );
        assert_eq!(
            entries[1],
            (ds.inode(), FileType::Directory, "..".to_string())
        );
        assert_eq!(
            entries[2],
            (
                metadata_inode,
                FileType::RegularFile,
                METADATA_FILE_NAME.to_string()
            )
        );
    }

    #[test]
    fn listing_rejects_unknown_inode() {
        let ds = sample_datasource();

        assert!(ds.entries_for_readdir(999).is_none());
    }
}
