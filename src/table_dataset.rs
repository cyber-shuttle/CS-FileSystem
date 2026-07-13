use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use fuser::{FileAttr, FileType, ReplyDirectory};

use crate::{directory_attr, regular_file_attr, InodeGenerator, VirtualDataSource};

const METADATA_FILE_NAME: &str = "metadata.json";

#[derive(Clone, Debug)]
pub struct TableEntry {
    pub id: String,
    pub metadata_json: String,
}

#[derive(Clone, Debug)]
struct TableEntryNode {
    metadata_inode: u64,
    id: String,
}

pub struct TableDataSource {
    name: String,
    inode: u64,
    entry_dirs: HashMap<u64, TableEntryNode>,
    entry_name_to_inode: HashMap<String, u64>,
    file_contents: HashMap<u64, String>,
}

impl TableDataSource {
    pub fn new(
        name: impl Into<String>,
        entries: Vec<TableEntry>,
        inode_gen: &mut InodeGenerator,
    ) -> Self {
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
                TableEntryNode {
                    metadata_inode,
                    id: entry.id,
                },
            );
        }

        TableDataSource {
            name: name.into(),
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
        let dataset_dir = Path::new(output_dir).join(&self.name);
        fs::create_dir_all(&dataset_dir)?;

        for id in self.entry_ids() {
            let entry_dir = dataset_dir.join(&id);
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

pub fn load_table_datasource(
    name: &str,
    table_path: &str,
    inode_gen: &mut InodeGenerator,
) -> TableDataSource {
    TableDataSource::new(name, parse_table(table_path), inode_gen)
}

pub fn parse_table(table_path: &str) -> Vec<TableEntry> {
    let file = fs::File::open(table_path).expect("Failed to open metadata table");
    parse_table_reader(BufReader::new(file))
}

fn parse_table_reader<R: BufRead>(reader: R) -> Vec<TableEntry> {
    let mut lines = reader.lines();
    let header_line = lines
        .next()
        .expect("metadata table is missing a header row")
        .expect("Failed to read metadata table header");
    let delimiter = if header_line.contains('\t') {
        '\t'
    } else {
        ','
    };
    let headers: Vec<String> = header_line.split(delimiter).map(str::to_string).collect();
    let mut entries = Vec::new();

    for line in lines {
        let line = line.expect("Failed to read metadata table row");
        if line.trim().is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(delimiter).collect();
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
        entries.push(TableEntry { id, metadata_json });
    }

    entries
}

impl VirtualDataSource for TableDataSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        "table"
    }

    fn inode(&self) -> u64 {
        self.inode
    }

    fn entry_count(&self) -> usize {
        self.entry_count()
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

    fn sample_entries() -> Vec<TableEntry> {
        parse_table_reader(Cursor::new(
            "entry_id\tfamily\ttemperature\nalpha\tgpcr\t310\nbeta\tmembrane\t323\n",
        ))
    }

    fn sample_datasource() -> TableDataSource {
        let mut inode_gen = InodeGenerator::new();
        TableDataSource::new("demo", sample_entries(), &mut inode_gen)
    }

    #[test]
    fn parses_tsv_rows_into_pretty_json_entries() {
        let entries = sample_entries();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "alpha");

        let metadata: serde_json::Value = serde_json::from_str(&entries[0].metadata_json).unwrap();
        assert_eq!(metadata["entry_id"], "alpha");
        assert_eq!(metadata["family"], "gpcr");
        assert_eq!(metadata["temperature"], "310");
    }

    #[test]
    fn parses_comma_separated_rows_too() {
        let entries = parse_table_reader(Cursor::new("entry_id,family\nalpha,gpcr\n"));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "alpha");
    }

    #[test]
    fn uses_configured_name_as_dataset_root() {
        let ds = sample_datasource();

        assert_eq!(ds.name(), "demo");
    }

    #[test]
    fn allocates_predictable_inodes_for_table_tree() {
        let ds = sample_datasource();

        assert_eq!(ds.inode(), 5);
        assert_eq!(ds.entry_count(), 2);
        assert_eq!(ds.entry_name_to_inode["alpha"], 6);
        assert_eq!(ds.entry_dirs[&6].metadata_inode, 7);
        assert_eq!(ds.entry_name_to_inode["beta"], 8);
        assert_eq!(ds.entry_dirs[&8].metadata_inode, 9);
    }

    #[test]
    fn lookup_finds_entry_directory_under_dataset_root() {
        let ds = sample_datasource();
        let attr = ds.lookup(ds.inode(), OsStr::new("alpha")).unwrap();

        assert_eq!(attr.ino, 6);
        assert_eq!(attr.kind, FileType::Directory);
    }

    #[test]
    fn read_returns_sliced_metadata_bytes() {
        let ds = sample_datasource();
        let entry_inode = ds.entry_name_to_inode["alpha"];
        let metadata_inode = ds.entry_dirs[&entry_inode].metadata_inode;
        let full_content = ds.file_contents[&metadata_inode].clone();

        assert_eq!(
            String::from_utf8(ds.read(metadata_inode, 0, 20).unwrap()).unwrap(),
            full_content[..20].to_string()
        );
        assert_eq!(
            ds.read(metadata_inode, 99_999, 10).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn root_listing_contains_sorted_entry_directories() {
        let ds = sample_datasource();
        let entries = ds.entries_for_readdir(ds.inode()).unwrap();

        assert_eq!(
            entries[0],
            (ds.inode(), FileType::Directory, ".".to_string())
        );
        assert_eq!(entries[2], (6, FileType::Directory, "alpha".to_string()));
        assert_eq!(entries[3], (8, FileType::Directory, "beta".to_string()));
    }
}
