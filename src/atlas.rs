use std::fs;
use std::io::{BufRead, BufReader};

pub struct AtlasEntry {
    pub id: String,
    pub metadata_json: String, // pre-serialized JSON for this entry
}

pub fn load_atlas(tsv_path: &str) -> Vec<AtlasEntry> {
    let file = fs::File::open(tsv_path).expect("Failed to open TSV");
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let header_line = lines.next().unwrap().unwrap();
    let headers: Vec<&str> = header_line.split('\t').collect();
    let mut entries = Vec::new();

    for line in lines {
        let line = line.unwrap();
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
