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

## Run

Build and mount the filesystem:

```bash
mkdir -p /tmp/atlas_mount
cargo run --release -- data/2024_11_18_ATLAS_info.tsv /tmp/atlas_mount
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
