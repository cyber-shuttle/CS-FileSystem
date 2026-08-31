# User Guide

`cs-filesystem` exposes dataset metadata as a read-only filesystem. You can use
it in three modes:

- `materialize`: write the tree to a normal directory.
- `fuse`: mount the tree as a live FUSE filesystem.
- `nfs`: serve the tree through a read-only NFS server.

## Install

Download the archive for your platform from GitHub Releases, then install the
binary:

```bash
tar -xzf cs-filesystem-<version>-<target>.tar.gz
cd cs-filesystem-<version>-<target>
chmod +x cs-filesystem
sudo install -m 0755 cs-filesystem /usr/local/bin/cs-filesystem
```

From source:

```bash
cargo build --release
sudo install -m 0755 target/release/cs-filesystem /usr/local/bin/cs-filesystem
```

## System Requirements

Linux:

```bash
sudo apt install libfuse3-dev libfuse-dev pkg-config
```

macOS:

```bash
brew install pkgconf
brew install --cask macfuse
```

macFUSE may require approval in System Settings before mounts work.

## Metadata Tables

ATLAS metadata is passed as the first table argument. Additional datasets can be
passed as `name=path` table specs.

The examples bundled with the repo are enough for a quick demo:

```text
examples/atlas_sample.tsv
examples/mdcath_sample.tsv
examples/memprotmd_sample.tsv
examples/gpcrmd_sample.tsv
```

For your own tables, use TSV or simple CSV. The first column becomes the entry
directory name, and every row becomes a `metadata.json` file.

## Materialize A Directory

This is the easiest mode for demos, inspection, and tools that only need a real
directory.

```bash
rm -rf /tmp/cs_sample_export
cs-filesystem materialize \
  examples/atlas_sample.tsv \
  mdcath=examples/mdcath_sample.tsv \
  memprotmd=examples/memprotmd_sample.tsv \
  gpcrmd=examples/gpcrmd_sample.tsv \
  /tmp/cs_sample_export
```

Browse it:

```bash
ls /tmp/cs_sample_export
cat /tmp/cs_sample_export/index.json
cat /tmp/cs_sample_export/registry.json
ls /tmp/cs_sample_export/atlas
cat /tmp/cs_sample_export/atlas/1r6w_A/metadata.json
```

## Mount With FUSE

Create a mount directory:

```bash
mkdir -p /tmp/cs_mount
```

Start the filesystem:

```bash
cs-filesystem fuse \
  examples/atlas_sample.tsv \
  mdcath=examples/mdcath_sample.tsv \
  memprotmd=examples/memprotmd_sample.tsv \
  gpcrmd=examples/gpcrmd_sample.tsv \
  /tmp/cs_mount
```

Leave that command running. In another terminal:

```bash
ls /tmp/cs_mount
cat /tmp/cs_mount/index.json
cat /tmp/cs_mount/datasets/alphafold_db/metadata.json
cat /tmp/cs_mount/memprotmd/1afo/metadata.json
```

Unmount on Linux:

```bash
fusermount -u /tmp/cs_mount
```

Unmount on macOS:

```bash
diskutil unmount /tmp/cs_mount
```

## Serve With NFS

Start the read-only NFS server:

```bash
cs-filesystem nfs \
  examples/atlas_sample.tsv \
  mdcath=examples/mdcath_sample.tsv \
  memprotmd=examples/memprotmd_sample.tsv \
  gpcrmd=examples/gpcrmd_sample.tsv \
  127.0.0.1:2049
```

Mounting the export depends on your host NFS client configuration and
permissions. Use this mode when another application or machine needs an NFS
view instead of a local FUSE mount.

## Filesystem Layout

```text
<mount-or-export>/
  index.json
  registry.json
  datasets/
    alphafold_db/
      metadata.json
  atlas/
    1r6w_A/
      metadata.json
  mdcath/
    1abcA00/
      metadata.json
  memprotmd/
    1afo/
      metadata.json
  gpcrmd/
    adrb2_active/
      metadata.json
```

`index.json` lists the datasets loaded in this run. `registry.json` contains
the broader built-in dataset registry. `datasets/` exposes the registry as
ordinary directories.

## Known Limitations

- The filesystem is read-only.
- Metadata tables are parsed as simple TSV or CSV; quoted CSV fields are not yet
  interpreted as structured CSV.
- The first column of each table must be a stable, filesystem-safe entry ID.
- Most registry datasets are metadata-only placeholders until dedicated
  connectors are implemented.
- FUSE mode requires host FUSE support and may need elevated setup on macOS.
- Large raw datasets are intentionally not downloaded during normal mounts.
