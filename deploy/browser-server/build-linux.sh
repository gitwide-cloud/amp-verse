#!/bin/sh
set -eu
source_root=${1:-/mnt/codex-builds/Software builds/amp-verse}
build_root=/srv/sager-work/sager/projects/amp-verse-browser
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_HOME=/srv/sager-work/sager/cache/cargo
export CARGO_TARGET_DIR=/srv/sager-work/sager/cache/amp-verse-target
mkdir -p "$build_root" "$CARGO_HOME" "$CARGO_TARGET_DIR"
# Copy includes uncommitted work, but never copies .git or build/dependency caches.
# No deletion: existing unrelated HGST files are preserved.
rsync -a --exclude=.git --exclude=android --exclude=test-output --exclude=build --exclude=.gradle --exclude=target --exclude=node_modules --exclude=.venv --exclude=dist "$source_root/" "$build_root/"
cd "$build_root"
cargo test --locked -p amp-cast
cargo build --locked --release -p amp-cast
printf '\nVerified executable: %s/release/amp-cast\n' "$CARGO_TARGET_DIR"
