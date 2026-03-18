#!/usr/bin/env bash
set -euo pipefail

cargo fmt --version >/dev/null
cargo clippy --version >/dev/null

echo "bootstrap complete"
