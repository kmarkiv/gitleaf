#!/bin/sh
# Build (if needed) and start gitleaf with the default settings:
#   projects root  $GITLEAF_ROOT   (default ~/Documents/gitleaf; legacy Codex/Overleaf is detected)
#   build cache    $GITLEAF_CACHE  (default ~/Library/Caches/gitleaf)
#   port           $GITLEAF_PORT   (default 8077)
#   engine         $GITLEAF_ENGINE (latexmk or tectonic; default: latexmk if installed, else tectonic)
# The binary verifies an existing instance before opening its browser.
# Set GITLEAF_REBUILD_UI=1 after editing the frontend to regenerate embedded assets.
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
RUST="$HERE/../.rust"
if [ -d "$RUST/cargo/bin" ]; then
  export CARGO_HOME="$RUST/cargo" RUSTUP_HOME="$RUST/rustup" PATH="$RUST/cargo/bin:$PATH"
fi
cd "$HERE"
if [ ! -f static/assets/app.js ] || [ -n "${GITLEAF_REBUILD_UI:-}" ]; then
  npm ci
  npm run build
fi
cargo build --release --locked --quiet
exec "$HERE/target/release/gitleaf" "$@"
