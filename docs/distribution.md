# Distribution

This project distributes `cs-filesystem` as platform-specific `.tar.gz`
archives. Each archive contains the binary, examples, and documentation.

## Supported Targets

- Linux x86_64
- macOS arm64
- macOS x86_64

The binary depends on the host FUSE stack when running in `fuse` mode:

- Linux: `libfuse3`/`libfuse`
- macOS: macFUSE

`materialize` and `nfs` mode do not require users to mount with FUSE, but the
binary is built with FUSE support.

## Local Package

Build and package the current host platform:

```bash
./scripts/package-release.sh
```

Outputs are written under:

```text
target/dist/
```

The archive name is:

```text
cs-filesystem-<version>-<target-triple>.tar.gz
```

## GitHub Releases

The release workflow lives at `.github/workflows/release.yml`.

To publish a release after reviewing local changes:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow will:

1. Build and test on Linux and macOS runners.
2. Package each platform binary.
3. Upload archives and SHA-256 checksum files to a GitHub Release.

The workflow can also be run manually with `workflow_dispatch`; manual runs build
artifacts but only tag pushes publish a GitHub Release.

## Pre-release Checklist

1. Update `version` in `Cargo.toml`.
2. Run `cargo test --locked`.
3. Run `cargo build --release`.
4. Run `./scripts/package-release.sh` locally.
5. Smoke test sample materialization:

```bash
rm -rf /tmp/cs_sample_export
./target/release/cs-filesystem materialize \
  examples/atlas_sample.tsv \
  mdcath=examples/mdcath_sample.tsv \
  memprotmd=examples/memprotmd_sample.tsv \
  gpcrmd=examples/gpcrmd_sample.tsv \
  /tmp/cs_sample_export

cat /tmp/cs_sample_export/index.json
cat /tmp/cs_sample_export/atlas/1r6w_A/metadata.json
```

6. Smoke test FUSE or NFS on at least one target host when possible.
7. Create and push a `vX.Y.Z` tag.
