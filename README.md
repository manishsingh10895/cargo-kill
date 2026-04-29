# Cargo Kill

### Remove `target` / `node_modules` (and framework caches) recursively from directories

## Installation

> `cargo install cargo-kill-all`

## Usage

> `cargo-kill-all /home/Documents/ -t 4 -p [npm/cargo]`

Use `-p npm` to clean Node projects and `-p cargo` to clean **cargo** projects.

### What gets cleaned

- **`-p cargo`** — `target/` next to every `Cargo.toml`.
- **`-p npm`** — `node_modules/` next to every `package.json`, plus framework
  build/cache directories detected by parsing the project's `package.json`
  dependencies:

  | Dependency       | Cache directories       |
  | ---------------- | ----------------------- |
  | `next`           | `.next`                 |
  | `nuxt` / `nuxt3` | `.nuxt`, `.output`      |
  | `@sveltejs/kit`  | `.svelte-kit`           |

  Each project shows up as a single selectable row whose size is the sum of
  all its detected directories; selecting it removes all of them. Projects
  with no recognized framework dep just clean `node_modules` as before.

### Including `.git`

Pass `--include-git` to also list each project's `.git` directory as a
deletion candidate. Only `.git` directories that sit alongside a detected
project (`Cargo.toml` or `package.json`) are considered. **This wipes the
local repository history** — you will see `.git` listed in the project's row
before confirming.

![Usage](./usage.png "Usage")

## This project is heavily inspired by [dnlmlr's](https://github.com/dnlmlr) crate

[cargo-clean-all](https://github.com/dnlmlr/cargo-clean-all)

### Demo

![Demo](./demo.png "Demo")

If you like this project, go check that out too
