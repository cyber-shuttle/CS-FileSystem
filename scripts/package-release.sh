#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

BIN_NAME="cs-filesystem"
VERSION="${VERSION:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"version":"\([^"]*\)".*/\1/p' | head -1)}"
TARGET_TRIPLE="${TARGET_TRIPLE:-$(rustc -vV | sed -n 's/^host: //p')}"
ARCHIVE_BASENAME="${BIN_NAME}-${VERSION}-${TARGET_TRIPLE}"
STAGING_DIR="target/dist/${ARCHIVE_BASENAME}"

cargo build --release

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"

cp "target/release/${BIN_NAME}" "$STAGING_DIR/"
cp README.md "$STAGING_DIR/"
cp -R docs "$STAGING_DIR/"
cp -R examples "$STAGING_DIR/"

cat > "$STAGING_DIR/INSTALL.md" <<'INSTALL'
# Install

Copy `cs-filesystem` somewhere on your PATH:

```bash
chmod +x cs-filesystem
sudo install -m 0755 cs-filesystem /usr/local/bin/cs-filesystem
```

See `docs/user-guide.md` for mount, materialize, and NFS usage.
INSTALL

mkdir -p target/dist
tar -C target/dist -czf "target/dist/${ARCHIVE_BASENAME}.tar.gz" "$ARCHIVE_BASENAME"

if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "target/dist/${ARCHIVE_BASENAME}.tar.gz" > "target/dist/${ARCHIVE_BASENAME}.tar.gz.sha256"
elif command -v sha256sum >/dev/null 2>&1; then
  sha256sum "target/dist/${ARCHIVE_BASENAME}.tar.gz" > "target/dist/${ARCHIVE_BASENAME}.tar.gz.sha256"
fi

echo "Wrote target/dist/${ARCHIVE_BASENAME}.tar.gz"
