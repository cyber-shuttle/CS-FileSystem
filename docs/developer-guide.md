# Developer Guide

`cs-filesystem` is a read-only filesystem for exposing Cybershuttle dataset
metadata as ordinary directories and JSON files.

## Repository Structure

```text
Cargo.toml                 Rust package metadata and dependencies
Cargo.lock                 Locked dependency graph
README.md                  Project overview and quick start
docs/                      Distribution, developer, and user documentation
examples/                  Tiny metadata tables for demos and tests
scripts/package-release.sh Local release archive builder
src/main.rs                CLI, FUSE filesystem, materialization path, NFS wiring
src/atlas.rs               ATLAS TSV parser and virtual data source
src/table_dataset.rs       Generic TSV/CSV-backed data source
src/dataset_registry.rs    Built-in official dataset registry
src/nfs_server.rs          Read-only NFS server implementation
```

The `data/` directory is intentionally local-only. Full metadata tables can be
large or subject to external access rules, so only small examples are committed.

## Architecture

The filesystem has one logical tree that can be exposed three ways:

- `fuse`: mount the virtual tree through FUSE.
- `materialize`: write the same tree to a normal directory.
- `nfs`: serve the same metadata tree through a read-only NFS server.

The root contains:

```text
index.json
registry.json
datasets/
atlas/
<optional table datasets>/
```

Each dataset root contains one directory per entry. Each entry directory contains
a `metadata.json` file.

```text
atlas/
  1r6w_A/
    metadata.json
```

## Core Interfaces

`src/main.rs` defines the shared data source interface:

```rust
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
```

FUSE uses this trait directly. `materialize` uses each data source's
materialization helper. NFS converts data sources into `NfsDataset` values and
serves them through `src/nfs_server.rs`.

Inodes are allocated by `InodeGenerator` in one pass while building data
sources. The root-level inode values in `main.rs` are reserved for
`/`, `hello.txt`, `index.json`, and `registry.json`; generated dataset inodes
start after those values.

## Metadata Registry

`src/dataset_registry.rs` contains the built-in registry of known AI-for-science
datasets. It exposes the registry in two forms:

- `registry.json`: one JSON file containing all registry entries.
- `datasets/<dataset-id>/metadata.json`: one directory per registry entry.

Registry entries include:

- stable ID
- display name
- domain
- access pattern
- implementation status
- source URL
- intended connector type
- notes

The registry is metadata-first. Planned or restricted datasets should be listed
without implying that raw data will be downloaded automatically.

## Connector Model

Current connectors are implemented as data sources:

- `AtlasDataSource`: parses ATLAS-specific TSV metadata.
- `TableDataSource`: parses generic TSV or CSV metadata, using the first column
  as the entry directory name.

The registry `connector` field describes the expected access pattern for future
connectors, such as `api`, `bundle`, `zenodo_metadata`, or `table`. A connector
should expose metadata first and avoid surprising bulk downloads during normal
mount or demo flows.

## Add A New Dataset From A Table

If the dataset can be represented by a TSV or CSV table, no code change is
required. Put a stable entry ID in the first column:

```text
entry_id	family	source
sample_001	protein	local-demo
```

Run:

```bash
cargo run --release -- materialize \
  examples/atlas_sample.tsv \
  my_dataset=/path/to/my_dataset.tsv \
  /tmp/cs_export
```

The output will include:

```text
/tmp/cs_export/my_dataset/sample_001/metadata.json
```

## Add A New Data Source In Code

Use a dedicated data source when a dataset needs custom parsing, remote API
metadata, caching, or non-table layout.

1. Add a module under `src/`.
2. Parse external metadata into stable entry records.
3. Allocate inodes with `InodeGenerator`.
4. Implement `VirtualDataSource`.
5. Add a `materialize` helper if the source should support static export.
6. Wire the source into `main.rs`.
7. Add or update a registry entry in `dataset_registry.rs`.
8. Add unit tests for parsing, lookup, read slicing, and directory listing.

Keep data sources read-only unless the project explicitly adds a write contract.

## Build And Test Locally

Install dependencies:

Linux:

```bash
sudo apt install cargo libfuse3-dev libfuse-dev pkg-config
```

macOS:

```bash
brew install pkgconf
brew install --cask macfuse
```

Build and test:

```bash
cargo test
cargo build --release
```

Run a sample export:

```bash
rm -rf /tmp/cs_sample_export
cargo run --release -- materialize \
  examples/atlas_sample.tsv \
  mdcath=examples/mdcath_sample.tsv \
  memprotmd=examples/memprotmd_sample.tsv \
  gpcrmd=examples/gpcrmd_sample.tsv \
  /tmp/cs_sample_export
```
