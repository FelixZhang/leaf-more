# AGENTS.md

Guidance for AI coding agents working in this repository.

## What this repo is

`leaf-more` is a personal-first fork of [RivoLink/leaf](https://github.com/RivoLink/leaf).
It exists to serve its owner first: features rejected by upstream (starting
with Kitty image rendering) live here. Upstream contribution is welcome but
secondary.

## Branch model

- `fork-main` — the integration branch and fork's default branch. It always
  contains all owner work. Deploy and backup happen here. **Never rebase it**;
  sync by merging:
  ```bash
  git fetch origin main
  git merge origin/main
  ```
- Short-lived `fix/*` / `feat/*` branches are based on **upstream `main`** so
  they can be contributed back cleanly:
  ```bash
  git checkout -b fix/xxx origin/main
  ```
  - Fix accepted by upstream → push and open a PR against `RivoLink/leaf`.
  - Fix rejected by upstream → merge the branch into `fork-main`.
- Fork-specific changes (rebrand, self-update targets, README fork notices)
  are committed **only** to `fork-main` and never mixed into branches meant
  for upstream PRs.
- When contributing upstream, follow upstream commit style: a single line, no
  scope, no body (e.g. `fix: skip kitty new-tab when no listen socket`).

## Remotes

- `origin` — upstream `RivoLink/leaf` (fetch + push).
- `fork` — `FelixZhang/leaf-more`; fetch via HTTPS, push via SSH.

## Workflow

1. New work: branch from `origin/main`.
2. If upstream-worthy: push the branch, open a PR upstream.
3. Either way, once the owner approves: merge into `fork-main` and push.
4. Deploy locally:
   ```bash
   cargo build --release
   # or install:
   cargo install --path /attic/hack/leaf-more --force
   ```

## Validation

Before finishing any change:

```bash
cargo +nightly fmt --all -- --check
cargo test
cargo +nightly clippy --all-targets --all-features -- -D warnings
cargo build --release
git diff --check
```

Note: stable toolchain may lack `rustfmt`/`clippy` components here; use the
nightly toolchain for fmt/clippy.

## Fork-specific behavior (do not "fix" these back to upstream)

- `kitty-images` rendering (Kitty TUI, default off, `i`/`I` toggle) — this is
  the feature upstream declined; keep it out of upstream PRs.
- `leaf --update` targets `FelixZhang/leaf-more`, not upstream.
- Package name is `leaf-more`; the binary remains `leaf`.
- The C-e editor path skips `kitty @ launch` unless `KITTY_LISTEN_ON` is set —
  this prevents a hard hang under PTY multiplexers (e.g. herdr) with kitty
  0.49.1+; do not remove the guard.
