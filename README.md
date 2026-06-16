# CS-FileSystem

This is a user-space filesystem for exposing Cybershuttle data sources.

This version loads ATLAS metadata from a TSV file and exposes each protein entry
as a directory containing a `metadata.json` file.

```text
/tmp/atlas_mount/
  atlas/
    1r6w_A/
      metadata.json
    2y44_A/
      metadata.json
```

FUSE mounting and NFS serving are supported directly. The same ATLAS tree can also be materialized to a real directory when a static export is useful.

## Requirements

### Linux

```bash
sudo apt install cargo
sudo apt install -y libfuse3-dev libfuse-dev pkg-config
```

### macOS

```bash
brew install pkgconf
brew install --cask macfuse
```

macFUSE may require approval in `System Settings -> Privacy & Security`.

## ATLAS TSV

Place the ATLAS metadata TSV somewhere local. The examples below assume the TSV is at:

```text
data/2024_11_18_ATLAS_info.tsv
```

The TSV is not committed to the repository. Create the `data` directory and copy
or download the file there:

```bash
mkdir -p data
cp /path/to/2024_11_18_ATLAS_info.tsv data/
```

## Run with FUSE

Build and mount the filesystem:

```bash
mkdir -p /tmp/atlas_mount
cargo run --release -- fuse data/2024_11_18_ATLAS_info.tsv /tmp/atlas_mount
```

Leave that command running while the filesystem is mounted.

In another terminal:

```bash
ls /tmp/atlas_mount
ls /tmp/atlas_mount/atlas | head
ls /tmp/atlas_mount/atlas/1r6w_A
cat /tmp/atlas_mount/atlas/1r6w_A/metadata.json
```

## Unmount

Linux:

```bash
fusermount -u /tmp/atlas_mount
```

macOS:

```bash
diskutil unmount /tmp/atlas_mount
```

## Materialize To Disk

Write the same ATLAS filesystem tree to a real directory:

```bash
rm -rf /tmp/atlas_export
cargo run --release -- materialize data/2024_11_18_ATLAS_info.tsv /tmp/atlas_export
ls /tmp/atlas_export/atlas | head
cat /tmp/atlas_export/atlas/1r6w_A/metadata.json
```

The `/tmp/atlas_export` directory can then be exported with the system NFS server.

## Run With NFS

Start the NFS server:

```bash
cargo run --release -- nfs data/2024_11_18_ATLAS_info.tsv 127.0.0.1:11111
```
In another terminal, mount it.

macOS:

```bash
mkdir -p /tmp/atlas_nfs
mount_nfs -o nolocks,vers=3,tcp,rsize=131072,actimeo=120,port=11111,mountport=11111 localhost:/ /tmp/atlas_nfs
ls /tmp/atlas_nfs/atlas | head
cat /tmp/atlas_nfs/atlas/1r6w_A/metadata.json
diskutil unmount /tmp/atlas_nfs
```
