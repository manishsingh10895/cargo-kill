#!/usr/bin/env bash
# Build a throwaway directory tree to try cargo-kill-all against.
#
# Usage:
#   scripts/make-sample-tree.sh [DIR]    # create at DIR (default: /tmp/ck-sample)
#   scripts/make-sample-tree.sh --clean [DIR]   # delete it
#
# After creating, try:
#   cargo-kill-all DIR -p npm -d
#   cargo-kill-all DIR -p cargo -d
# Drop -d to actually delete (you'll be prompted).

set -euo pipefail

ROOT="${2:-${1:-/tmp/ck-sample}}"

if [[ "${1:-}" == "--clean" ]]; then
    rm -rf "$ROOT"
    echo "removed $ROOT"
    exit 0
fi

if [[ -e "$ROOT" ]]; then
    echo "error: $ROOT already exists. Re-run with --clean first." >&2
    exit 1
fi

# A file of N kilobytes of zeros. Cheap to create, real bytes on disk.
fill() {
    local path="$1" kb="$2"
    mkdir -p "$(dirname "$path")"
    dd if=/dev/zero of="$path" bs=1024 count="$kb" status=none
}

# --- npm projects ---

# Next.js app
mkdir -p "$ROOT/web/next-app"
cat > "$ROOT/web/next-app/package.json" <<'JSON'
{"name":"next-app","dependencies":{"next":"14","react":"18"}}
JSON
fill "$ROOT/web/next-app/node_modules/.bin/placeholder" 2048
fill "$ROOT/web/next-app/.next/cache/placeholder"        1024

# SvelteKit app (dep declared in devDependencies)
mkdir -p "$ROOT/web/svelte-app"
cat > "$ROOT/web/svelte-app/package.json" <<'JSON'
{"name":"svelte-app","devDependencies":{"@sveltejs/kit":"2","vite":"5"}}
JSON
fill "$ROOT/web/svelte-app/node_modules/placeholder"   512
fill "$ROOT/web/svelte-app/.svelte-kit/placeholder"    256

# Nuxt 3 app (two cache dirs)
mkdir -p "$ROOT/web/nuxt-app"
cat > "$ROOT/web/nuxt-app/package.json" <<'JSON'
{"name":"nuxt-app","dependencies":{"nuxt":"3"}}
JSON
fill "$ROOT/web/nuxt-app/node_modules/placeholder"  256
fill "$ROOT/web/nuxt-app/.nuxt/placeholder"          64
fill "$ROOT/web/nuxt-app/.output/placeholder"       128

# Plain node project (no recognized framework dep)
mkdir -p "$ROOT/web/plain-node"
cat > "$ROOT/web/plain-node/package.json" <<'JSON'
{"name":"plain-node","dependencies":{"express":"4"}}
JSON
fill "$ROOT/web/plain-node/node_modules/placeholder" 128

# Malformed package.json — should degrade to node_modules only, no crash
mkdir -p "$ROOT/web/broken-json"
printf '{"dependencies":{"next' > "$ROOT/web/broken-json/package.json"
fill "$ROOT/web/broken-json/node_modules/placeholder" 64

# --- cargo projects ---

mkdir -p "$ROOT/rust/crate-a"
cat > "$ROOT/rust/crate-a/Cargo.toml" <<'TOML'
[package]
name = "crate-a"
version = "0.1.0"
edition = "2021"
TOML
fill "$ROOT/rust/crate-a/target/debug/placeholder" 4096
fill "$ROOT/rust/crate-a/src/main.rs" 1

mkdir -p "$ROOT/rust/crate-b"
cat > "$ROOT/rust/crate-b/Cargo.toml" <<'TOML'
[package]
name = "crate-b"
version = "0.1.0"
edition = "2021"
TOML
fill "$ROOT/rust/crate-b/target/release/placeholder" 1024
fill "$ROOT/rust/crate-b/src/main.rs" 1

# A cargo project with no target/ — should not appear in npm or cargo runs
mkdir -p "$ROOT/rust/crate-no-build/src"
cat > "$ROOT/rust/crate-no-build/Cargo.toml" <<'TOML'
[package]
name = "crate-no-build"
version = "0.1.0"
edition = "2021"
TOML
fill "$ROOT/rust/crate-no-build/src/main.rs" 1

cat <<EOF

Sample tree created at: $ROOT

Try it:
  cargo-kill-all $ROOT -p npm   -d   # dry run, npm projects + framework caches
  cargo-kill-all $ROOT -p cargo -d   # dry run, cargo projects

Expected npm rows:
  next-app       node_modules, .next
  svelte-app     node_modules, .svelte-kit
  nuxt-app       node_modules, .nuxt, .output
  plain-node     node_modules
  broken-json    node_modules         (malformed package.json, graceful fallback)

Expected cargo rows:
  crate-a        target
  crate-b        target

Clean up when done:
  $0 --clean $ROOT
EOF
