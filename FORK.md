# Fork features

`leaf-more` extends [`RivoLink/leaf`](https://github.com/RivoLink/leaf) with
the features below. See [README.md](README.md#about-this-fork) for what this
fork is and why it exists; this page only tracks what it adds on top of
upstream `main`.

## Kitty image rendering

- Local PNG, JPEG, GIF, and WebP images render inline in the Kitty-protocol
  TUI (kitty terminal).
- Images decode in a background worker — cancellable, panic-isolated, and
  bounded (20 MiB per file, 16384 px, 128 MiB allocations) — so the UI never
  blocks on a large image.
- Placement is scroll-aware, with aspect-ratio-based heights capped at
  12–40 rows.
- Off by default. Enable with `kitty-images = true` in the config, the
  `LEAF_KITTY_IMAGES` environment variable, or the `i` / `I` session toggle.
- With images disabled, rendering is identical to upstream: plain alt text,
  no reserved geometry.
- Sources that cannot render (remote URLs, SVG, images inside tables or
  footnotes, `--inline` output) keep readable alt-text placeholders.

## Status bar image indicator

- A `▨ img` section appears in the status bar while image rendering is
  active, next to the watch indicator.

## Editor hang fix

- Opening an editor with `Ctrl+E` could hang forever under kitty 0.49.1 when
  the pane lives inside a terminal workspace manager such as herdr: `kitty @`
  blocks waiting for a DCS reply that cannot cross the multiplexer. The editor
  integration now only attempts `kitty @` when `KITTY_LISTEN_ON` is set.
- Proposed upstream as
  [#311](https://github.com/RivoLink/leaf/pull/311)
  (fixes [#310](https://github.com/RivoLink/leaf/issues/310)).

## Picker page jumps

- `PageUp` / `PageDown` and `Ctrl+D` / `Ctrl+U` jump 10 entries in the file
  browser, the fuzzy finder, and the file-history pickers, clamping at the
  edges.
- Proposed upstream as
  [#318](https://github.com/RivoLink/leaf/pull/318)
  (issue [#319](https://github.com/RivoLink/leaf/issues/319)).

## In development

These live on the working tree, not yet in a release:

- `/` quick filter in the file browser — a vim-style explicit filter pass,
  so letter hotkeys (`j`/`k`/`h`/`q`/`P`) keep their browse meaning until
  filtering is requested.
- `Ctrl+R` path-match toggle in the fuzzy finder — file-name matching by
  default; toggling matches the full relative path instead.

## Install

```sh
cargo install --git https://github.com/FelixZhang/leaf-more
```

The binary is still called `leaf`. For branch layout and contribution
workflow, see [AGENTS.md](AGENTS.md).
