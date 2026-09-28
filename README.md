<p align="center">
  <strong>leaf-more</strong><br>
  A small fork of
  <a href="https://github.com/RivoLink/leaf">leaf</a>
  with a few extra features, starting with Kitty image rendering.
</p>

<p align="center">
  <img src="images/preview.png" alt="leaf-more" width="710px" /><br>
  <sub>See more screenshots in the <a href="demo/README.md">features</a> demo</sub>
</p>

## About this fork

`leaf-more` is an independent fork of
[`RivoLink/leaf`](https://github.com/RivoLink/leaf). It is not affiliated with
or endorsed by the upstream project.

Upstream remains the canonical `leaf`. Thanks to
[@RivoLink](https://github.com/RivoLink) and all upstream contributors for the
original project and its excellent terminal experience. This fork exists for
optional additions that fall outside upstream's current scope, starting with
local Kitty image rendering. The upstream author's decision not to include
image preview is respected here.

The executable is still called `leaf`, so an existing workflow can use this
fork without changing commands. Use one fork at a time: don't expect an
upstream binary to render images, and don't expect this fork to follow
upstream-only changes automatically.

## Install

Install from source with Cargo:

```bash
cargo install --git https://github.com/FelixZhang/leaf-more
```

This installs the `leaf` executable from `leaf-more`.

**Verify the installation**

```bash
leaf --version
```

> To install upstream `leaf`, use the installation instructions in the
> [upstream repository](https://github.com/RivoLink/leaf). The npm, Homebrew,
> Scoop, and AUR packages documented there are upstream packages, not
> `leaf-more`.

## Update

After a new `leaf-more` commit or release:

```bash
cargo install --git https://github.com/FelixZhang/leaf-more --force
```

If a published `leaf-more` release is available, `leaf --update` can download
the matching asset and verify its SHA256 checksum. Until then, use the Cargo
command above.

## Usage

```bash
# Open a Markdown file
leaf TESTING.md

# Watch mode: reloads automatically on save
leaf --watch TESTING.md
leaf -w TESTING.md

# Open the fuzzy Markdown picker
leaf

# Open the fuzzy Markdown picker with a pre-filled filter
# keyword: alphanumeric, max 15 chars
leaf --fuzzy keyword
leaf keyword

# Open the classic directory browser picker
leaf --picker

# Open the fuzzy Markdown picker, then watch the selected file
leaf -w

# Open the classic directory browser picker, then watch the selected file
leaf -w --picker

# Open a dash-prefixed filename
leaf -- -notes.md

# Open the file history picker
leaf --history
leaf -H

# Open the most recent from file history
leaf --last
leaf -l

# Combine with watch mode
leaf --last --watch

# Pick a color theme
leaf --theme forest TESTING.md

# Set the external editor
leaf --editor nvim TESTING.md
leaf -e code TESTING.md

# Cap the content width (min: 20)
leaf --width 100 TESTING.md

# Stream Markdown from another CLI tool
claude "explain Rust lifetimes" | leaf

# Preview a local file through stdin
cat TESTING.md | leaf
```

## Inline Images (Kitty TUI)

When running the interactive TUI in [Kitty](https://sw.kovidgoyal.net/kitty/), Leaf can render local Markdown images with Kitty's graphics protocol. Image rendering is **disabled by default**. Set `kitty-images = true` in `config.toml`, set `LEAF_KITTY_IMAGES=1`, or press `i` / `I` in the TUI to enable it for the current session:

```markdown
![Architecture diagram](images/architecture.png)
```

Supported formats are PNG, JPEG, GIF, and WebP. Animated images display their first frame. Relative paths are resolved from the Markdown file's directory; files read from stdin are resolved from the current working directory.

Images are decoded and encoded in a background worker, then rendered inside a document block whose height is based on the image aspect ratio, so scrolling, search, line numbers, TOC resizing, reloads, and terminal resizing keep working. Local image dimensions determine the reserved height, clamped to 12–40 rows; unavailable dimensions use the 12-row minimum. Missing, oversized, remote, SVG, and unsupported images keep a readable image placeholder with their alt text and source.

When image rendering is off, Leaf uses its normal upstream Markdown layout: only the alt text is shown, with no image border and no extra reserved rows. Pressing `i/I` reparses the document to create image blocks; turning it off reparses again to remove them.

This first stage intentionally supports only Kitty in the interactive TUI. Other terminals and `--inline` output show the same text placeholder and never receive Kitty control sequences.

## Inline Mode

Render Markdown directly to **stdout** without the interactive TUI:

```bash
# Render to terminal with colors
leaf --inline README.md

# Force plain text, no ANSI codes (no colors)
leaf --inline plain README.md

# Force ANSI colors even when piping
leaf --inline ansi README.md

# Set a specific width
leaf --inline 60 README.md
leaf --inline ansi:60 README.md

# Pipe from stdin
cat README.md | leaf --inline

# Use as a fzf preview
fzf --preview 'leaf --inline ansi {}'
fzf --preview 'leaf --inline ansi:$FZF_PREVIEW_COLUMNS {}'
```

## Shell Completions

Enable Tab completion for all arguments:

```bash
leaf --auto-complete
```

Supports **bash**, **zsh**, **fish**, **Nushell**, and **PowerShell**. Restart your shell to activate.

## Vim Integration
Add the following to your `~/.vimrc` to preview the current Markdown file in a vertical split:

```vim
" Preview the current Markdown file in a vertical split using leaf
nnoremap <Leader>md :vertical botright terminal leaf -w %<CR>
```

Once added, use `\md` to open a live preview. To switch focus back to the Markdown buffer, press `Ctrl+w,h`.

## Configuration

Set default values for **theme**, **editor**, **watch** mode, **extra** file types, and more via `config.toml`:

```bash
leaf --config
```

This opens the configuration file in your editor. If the file does not exist yet, **leaf** creates it with documented defaults.

```toml
theme = "ocean"            # arctic, forest, ocean, solarized-dark, or a custom theme file
editor = "nano"            # any editor in PATH
watch = false              # auto-reload when opening a file
kitty-images = false       # enable local images in the Kitty TUI (toggle with i/I)
width = 80                 # maximum content width (min: 20, default: terminal width)
extras = ["txt", "rs"]     # extra file types shown in the picker
file-picker-width = "75%"  # width of the file picker (fuzzy + file browser)
main-line-numbers = false  # show line numbers in the main document by default
code-line-numbers = true   # show line numbers inside fenced code blocks
tab-title-length = -1      # terminal tab title truncation (min: 20, -1: no truncation)
file-history-length = 0    # recent file history length (0 disables, max: 50)
hyper-link-prefix = "#"    # single character before link text ("" for none)
```

To reset the configuration to defaults:

```bash
leaf --config reset
```

To remove the configuration directory and all its contents:

```bash
leaf --config remove
```

All settings are optional. CLI arguments always take priority. See [`config.toml`](config.toml) for details.

## Open in Editor

Press `Ctrl+E` to open the current file in the configured editor.

Use `{$line}` in `editor` to open at the line currently visible in **leaf**:

```toml
editor = 'nano +{$line}'
editor = 'nvim +{$line} +"normal! zz"'
```

For further customization, `{$path}` is also available for the file path:

```toml
editor = 'code -g {$path}:{$line}'
```

Without `{$line}`, the editor opens at the top of the file.

## Extra Files

Non-Markdown files can be listed in the file picker by adding their extensions to `config.toml`:

```toml
extras = ["txt", "csv", "rs", "java", "json", "yaml"]
```

Code files get syntax highlighting; text files are rendered as plain Markdown.

Any file can also be opened directly from the command line, regardless of the `extras` setting:

```bash
leaf main.rs
```

Browse and preview code files with fzf:

```bash
find . -name '*.rs' | fzf --preview 'leaf --inline ansi {}'
```

## Custom Themes

Create a `.toml` file that inherits from a built-in theme and overrides specific colors:

```toml
theme = "/path/to/custom-theme.toml"
```

Relative paths are resolved from the config file directory.

```toml
# custom-theme.toml
base = "ocean"
syntax = "base16-ocean.dark"

[ui]
content_bg = "#282828"
toc_accent = "#fe8019"

[markdown]
text = "#ebdbb2"
heading_1 = "#fabd2f"
```

See [`gruvbox.toml`](gruvbox.toml) for a complete example with all available color keys.

## Keybindings

| Key | Action | Key | Action |
|---|---|---|---|
| `j` / `↓` | Scroll down | `?` | Show help popup |
| `k` / `↑` | Scroll up | `p` | Show file path |
| `d` / PgDn | Page down (20 lines) | `Shift+L` | Toggle line numbers |
| `u` / PgUp | Page up (20 lines) | `Shift+T` | Open theme picker |
| `g` / Home | Top | `Shift+E` | Open editor picker |
| `G` / End | Bottom | `Shift+P` | Open file browser |
| `1-9` / `0+1-9` | Jump / reverse jump (TOC) | `Shift+M` | Toggle mouse capture |
| `J/K` / `U/D` | Navigate TOC | `Ctrl+P` | Open fuzzy picker |
| `y/Y` / `c/C` | Focus code block | `Ctrl+H` | Open file history picker |
| `Ctrl+L` / `:` | Go to line | `Ctrl+E` | Open in editor |
| `Ctrl+F` / `/` | Find | `Ctrl+Click` | Open link |
| `n` / `N` | Next / prev match | `Double-Click` (link) | Copy link |
| `w` | Toggle watch mode | `Double-Click` (code) | Copy code block |
| `r` | Force reload (watch mode) | `Shift+Drag` | Select text |
| `t` | Toggle TOC sidebar | `Option+Drag` | Select text (iTerm2) |
| `i` / `I` | Toggle Kitty images | `q` | Quit |

## Features

- **Live preview** : *Watch mode with automatic reload and visual feedback*.
- **File picker** : *Fuzzy Markdown picker, directory browser, and watch after selection*.
- **File history** : *Recently opened files stored in `history.toml`, picker via `Ctrl+H` or `leaf --history`*.
- **Editor integration** : *Open the current file in your preferred editor*.
- **Frontmatter support** : *YAML frontmatter rendered as a table (horizontal or vertical based on key count)*.
- **Rich Markdown rendering** : *Tables, lists, blockquotes, rules, bold, italic, and strikethrough*.
- **Kitty images** : *Optional local PNG, JPEG, GIF, and WebP rendering in the interactive Kitty TUI, disabled by default and toggled with `i/I`*.
- **GitHub extras** : *Alert callouts, task list checkboxes, and `==mark==` text highlighting*.
- **Inline HTML tags** : *Common inline formatting tags (`<b>`, `<i>`, `<u>`, ...) rendered with proper styling*.
- **Extra file types** : *Open any file; code files get syntax highlighting, text files render as Markdown*.
- **Syntax highlighting** : *Common aliases like `py`, `cpp`, `json`, `toml`, `ps1`, `dockerfile`*.
- **Line numbers** : *Toggle display with `Shift+L`, jump to a line with `Ctrl+L` or `:`*.
- **LaTeX support** : *Inline, block, and `latex` / `tex` code blocks rendered as formulas*.
- **Mermaid diagrams** : *`mermaid` code blocks rendered as ASCII diagrams*.
- **Clickable links** : *`Ctrl+Click` to open, double-click to copy, hover feedback*.
- **Code block interactions** : *Focus and copy with `y/Y` / `c/C`, or double-click on a block*.
- **Mouse capture** : *`Shift+M` to toggle mouse capture and let the terminal handle selection*.
- **Navigation** : *TOC sidebar, active section tracking, heading jumps, and search*.
- **Terminal UX** : *Theme picker, help popup, file path popup, mouse and keyboard support*.
- **Custom themes** : *TOML theme files inheriting from built-in presets with color overrides*.
- **Inline mode** : *Render to stdout with `--inline` for pipes and fzf previews*.
- **Shell completions** : *Tab completion for bash, zsh, fish, Nushell, and PowerShell via `leaf --auto-complete`*.
- **CLI friendly** : *stdin support and `leaf --update` with SHA256 verification*.

## Typical AI Workflow

```bash
# Terminal 1: generate the file
aichat "..." > notes.md

# Terminal 2: live watch
leaf --watch notes.md
```

## Troubleshooting

### Images remain placeholders

- Image rendering is disabled by default: set `kitty-images = true`, use `LEAF_KITTY_IMAGES=1`, or press `i/I` in the TUI.
- Confirm the interactive TUI is running in Kitty, not `--inline` mode.
- Use a local PNG, JPEG, GIF, or WebP path; HTTP(S), data URLs, and SVG are not downloaded or rendered.
- Relative image paths are resolved from the Markdown file's directory.
- Images larger than 20 MiB, wider or taller than 16,384 pixels, or requiring more than 128 MiB to decode are rejected.
- Images in tables and footnotes remain text placeholders in this first stage.

### Windows: missing Visual C++ runtime

If `leaf.exe` does not start on Windows or reports a missing MSVC runtime, install the latest supported Microsoft Visual C++ Redistributable from Microsoft Learn:

- https://learn.microsoft.com/fr-fr/cpp/windows/latest-supported-vc-redist?view=msvc-170

Direct download for the latest supported **X64** Microsoft Visual C++ Redistributable:

- https://aka.ms/vc14/vc_redist.x64.exe

For `leaf-windows-x86_64.exe`, the relevant package is the latest supported **X64** Visual C++ v14 Redistributable.

### Windows: update or file replacement error

If `leaf --update` fails on Windows with an error about replacing, renaming, or writing `leaf.exe`, the running executable was likely locked by the OS.

Close any terminal session still running `leaf`, then reinstall from source:

```bash
cargo install --git https://github.com/FelixZhang/leaf-more --force
```

### Windows: auto-complete execution policy error

If PowerShell reports that running scripts is disabled on this system after `leaf --auto-complete`, allow local scripts and restart PowerShell:

```powershell
Set-ExecutionPolicy RemoteSigned -Scope CurrentUser
```

## Uninstall

Remove the `leaf` configuration, shell auto-completion, and Cargo-installed
binary:

```bash
leaf --config remove
leaf --auto-complete remove
cargo uninstall leaf-more
```

If you installed upstream `leaf` from another package manager, use that
package manager's uninstall command instead.

## Contributors

`leaf-more` builds on the work of [@RivoLink](https://github.com/RivoLink) and
all [upstream contributors](https://github.com/RivoLink/leaf/graphs/contributors).

## Support

Fork-specific issues and contributions are welcome in this repository. For
behavior that also exists in upstream `leaf`, consider reporting it upstream
first so the canonical project benefits.

## License

This project is licensed under the MIT License.

See the [LICENSE](LICENSE) file for details.
