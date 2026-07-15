# CS-FileSystem

This is a user-space filesystem for exposing Cybershuttle data sources.

This version loads ATLAS metadata from a TSV file and can also load mdCATH,
MemProtMD, and GPCRmd metadata from TSV or simple CSV files. Each dataset entry
is exposed as a directory containing a `metadata.json` file.

```text
/tmp/atlas_mount/
  index.json
  registry.json
  datasets/
    alphafold_db/
      metadata.json
    dandi/
      metadata.json
  atlas/
    1r6w_A/
      metadata.json
    2y44_A/
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

FUSE mounting and NFS serving are supported directly. The same dataset tree can
also be materialized to a real directory when a static export is useful.

`index.json` describes the datasets loaded in the current filesystem instance.
`registry.json` describes the broader official dataset registry and how each
dataset should be connected. The same official list is also exposed as the
`datasets/` directory, where each dataset has a `metadata.json` file.

## Official Dataset Registry

The filesystem includes an official registry for the AI-for-science datasets
being considered across domains. This is intentionally metadata-first: many of
the public datasets are too large to download during a normal demo, and some
medical/neuroscience datasets require accounts, approvals, or terms acceptance.

The registry groups datasets by access pattern:

```text
public_api
  AlphaFold DB
  DANDI
  Allen Institute datasets

huggingface / github / zenodo / tfds
  PDEArena
  PDEBench
  SPICE
  QM9

bulk_download / bulk_bundle
  Open Catalyst
  MSA databases for AlphaFold/OpenFold
  MICrONS

mixed or restricted access
  PhysioNet
  CheXpert
  SleepData / NSRR
  CRCNS
```

The currently loaded demo datasets are still exposed as normal directories, while
the broader registry is exposed through both `registry.json` and `datasets/`:

```bash
cat /tmp/cs_sample_export/registry.json
ls /tmp/cs_sample_export/datasets
cat /tmp/cs_sample_export/datasets/alphafold_db/metadata.json
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

## Metadata Tables

Place the ATLAS metadata TSV somewhere local. If you also want mdCATH,
MemProtMD, or GPCRmd, place their metadata tables somewhere local too. The
examples below assume these paths:

```text
data/2024_11_18_ATLAS_info.tsv
data/mdcath_metadata.tsv
data/memprotmd_metadata.tsv
data/gpcrmd_metadata.tsv
```

The data files are not committed to the repository. Create the `data` directory
and copy or download the files there:

```bash
mkdir -p data
cp /path/to/2024_11_18_ATLAS_info.tsv data/
cp /path/to/mdcath_metadata.tsv data/
cp /path/to/memprotmd_metadata.tsv data/
cp /path/to/gpcrmd_metadata.tsv data/
```

Tiny sample tables are also included under `examples/` so the filesystem can be
demoed without the full public metadata files.

## Quick Demo With Samples

Materialize the sample datasets to a normal directory:

```bash
rm -rf /tmp/cs_sample_export
cargo run --release -- materialize examples/atlas_sample.tsv examples/mdcath_sample.tsv examples/memprotmd_sample.tsv examples/gpcrmd_sample.tsv /tmp/cs_sample_export
```

The generic table form can expose any TSV/CSV metadata dataset by passing
`name=path` pairs:

```bash
rm -rf /tmp/cs_sample_export
cargo run --release -- materialize examples/atlas_sample.tsv mdcath=examples/mdcath_sample.tsv memprotmd=examples/memprotmd_sample.tsv gpcrmd=examples/gpcrmd_sample.tsv /tmp/cs_sample_export
```

Inspect the generated tree:

```bash
ls /tmp/cs_sample_export
cat /tmp/cs_sample_export/index.json
cat /tmp/cs_sample_export/registry.json
cat /tmp/cs_sample_export/datasets/alphafold_db/metadata.json
cat /tmp/cs_sample_export/atlas/1r6w_A/metadata.json
cat /tmp/cs_sample_export/mdcath/1abcA00/metadata.json
cat /tmp/cs_sample_export/memprotmd/1afo/metadata.json
cat /tmp/cs_sample_export/gpcrmd/adrb2_active/metadata.json
```

## Run with FUSE

Build and mount the filesystem:

```bash
mkdir -p /tmp/atlas_mount
cargo run --release -- fuse data/2024_11_18_ATLAS_info.tsv /tmp/atlas_mount
```

Leave that command running while the filesystem is mounted.

To expose ATLAS, mdCATH, MemProtMD, and GPCRmd together:

```bash
mkdir -p /tmp/cs_mount
cargo run --release -- fuse data/2024_11_18_ATLAS_info.tsv data/mdcath_metadata.tsv data/memprotmd_metadata.tsv data/gpcrmd_metadata.tsv /tmp/cs_mount
```

Or use explicit `name=path` table dataset arguments:

```bash
mkdir -p /tmp/cs_mount
cargo run --release -- fuse data/2024_11_18_ATLAS_info.tsv mdcath=data/mdcath_metadata.tsv memprotmd=data/memprotmd_metadata.tsv gpcrmd=data/gpcrmd_metadata.tsv /tmp/cs_mount
```

In another terminal:

```bash
ls /tmp/cs_mount
cat /tmp/cs_mount/index.json
cat /tmp/cs_mount/registry.json
ls /tmp/cs_mount/atlas | head
ls /tmp/cs_mount/mdcath | head
ls /tmp/cs_mount/memprotmd | head
ls /tmp/cs_mount/gpcrmd | head
cat /tmp/cs_mount/mdcath/1abcA00/metadata.json
cat /tmp/cs_mount/memprotmd/1afo/metadata.json
cat /tmp/cs_mount/gpcrmd/adrb2_active/metadata.json
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

Write ATLAS, mdCATH, MemProtMD, and GPCRmd together:

```bash
rm -rf /tmp/cs_export
cargo run --release -- materialize data/2024_11_18_ATLAS_info.tsv data/mdcath_metadata.tsv data/memprotmd_metadata.tsv data/gpcrmd_metadata.tsv /tmp/cs_export
ls /tmp/cs_export/atlas | head
cat /tmp/cs_export/index.json
cat /tmp/cs_export/registry.json
ls /tmp/cs_export/mdcath | head
ls /tmp/cs_export/memprotmd | head
ls /tmp/cs_export/gpcrmd | head
cat /tmp/cs_export/mdcath/1abcA00/metadata.json
cat /tmp/cs_export/memprotmd/1afo/metadata.json
cat /tmp/cs_export/gpcrmd/adrb2_active/metadata.json
```

The materialized directory can then be exported with the system NFS server.

## Run With NFS

Start the NFS server:

```bash
cargo run --release -- nfs data/2024_11_18_ATLAS_info.tsv 127.0.0.1:11111
```

To serve ATLAS, mdCATH, MemProtMD, and GPCRmd together:

```bash
cargo run --release -- nfs data/2024_11_18_ATLAS_info.tsv data/mdcath_metadata.tsv data/memprotmd_metadata.tsv data/gpcrmd_metadata.tsv 127.0.0.1:11111
```

In another terminal, mount it.

macOS:

```bash
mkdir -p /tmp/atlas_nfs
mount_nfs -o nolocks,vers=3,tcp,rsize=131072,actimeo=120,port=11111,mountport=11111 localhost:/ /tmp/atlas_nfs
ls /tmp/atlas_nfs/atlas | head
cat /tmp/atlas_nfs/index.json
cat /tmp/atlas_nfs/registry.json
ls /tmp/atlas_nfs/mdcath | head
ls /tmp/atlas_nfs/memprotmd | head
ls /tmp/atlas_nfs/gpcrmd | head
cat /tmp/atlas_nfs/mdcath/1abcA00/metadata.json
cat /tmp/atlas_nfs/memprotmd/1afo/metadata.json
cat /tmp/atlas_nfs/gpcrmd/adrb2_active/metadata.json
diskutil unmount /tmp/atlas_nfs
```
