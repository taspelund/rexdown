# Rexdown

A live, read-only Markdown viewer for Rex, with an optional Amp plugin. Keep a
design document or plan beside your conversation while edits appear automatically.
The table of contents is **hidden by default**; press `t` to expand/collapse it.

With the plugin installed, ask Amp: **“Open README.md in a Rex pane.”** The
`rex_open_markdown` tool opens a split to the right without taking keyboard focus.
Reopening the same file from the same thread and Rex window reuses the preview.

Rexdown is an experimental, independent project, not an official Rex or Amp
integration. The plugin directory and tool name remain `rex-markdown` and
`rex_open_markdown` so existing installations continue to work.

## Get the source

```sh
git clone https://github.com/taspelund/rexdown.git
cd rexdown
```

Or use `jj git clone https://github.com/taspelund/rexdown.git rexdown` and open
that checkout. Neither jj nor Git is needed at runtime.

## Build and install

Install [Rust through rustup](https://rustup.rs/) and a native linker (on macOS,
`xcode-select --install`; on Debian/Ubuntu, `build-essential`). From the checkout:

```sh
cargo install --path . --locked
rexdown --version
rexdown --check
```

The checkout pins Rust 1.98.0 in `rust-toolchain.toml`. Cargo installs the standalone
binary in `~/.cargo/bin`; ensure that directory is on PATH in the terminal that
starts Amp. `--check` prints the executable's absolute path without opening a TUI.
Building needs network access to download crates. Running the installed binary
does **not** require Python, uv, Cargo, the checkout, or network access.

The viewer works in a terminal without Rex or Amp. For Rex splits, install
[Rex](https://www.superlogical.com/rex) locally with `rex` on PATH. The optional
agent tool also requires [Amp](https://ampcode.com/docs):

```sh
rex whoami
amp --version  # optional for standalone use
```

Tested on macOS with Rex CLI `0.0.1792158932-g88fd41`. Other platforms and older Rex
versions have not been integration-tested. The plugin requires a local Unix-socket Rex
connection and the CLI's `split`, `block inspect`, `block ls`, and `block call
process` commands.

## Install the Amp plugin on this machine

After installing the binary, copy the adapter below. Amp supplies its TypeScript
runtime; there is no Node/npm installation or build step. The guard avoids
overwriting an existing install or development loader.

```sh
destination="$HOME/.config/amp/plugins/rex-markdown"
if [ -e "$destination" ] || [ -L "$destination" ] ||
   [ -e "$destination.ts" ] || [ -L "$destination.ts" ]; then
  printf 'Already installed at %s (or .ts); see Update below.\n' "$destination"
else
  mkdir -p "$destination" &&
    cp ./rex-markdown/index.ts "$destination/index.ts" &&
    cp ./LICENSE "$destination/LICENSE"
fi
```

1. Start Amp **inside a Rex pane**. If Amp is already running, ask it to reload
   plugins (or restart Amp).
2. Ask: **“Open README.md in a Rex pane.”** File paths are relative to Amp's
   workspace root, not this plugin's installation directory. An absolute path
   works too.
3. A rendered document should appear on the right, with a `LIVE` status and no
   ToC sidebar. Edit and save the original file; the preview should refresh
   within about a second. Ask to open it again to check that the pane is reused.

The tool can also take `anchor`, a full Rex block ID, when Amp was not launched
inside Rex. Use `rex block ls --json` to identify the intended pane explicitly.
Do not install both the directory plugin and the development loader.

## Use it without Amp

Run the viewer in your current terminal:

```sh
rexdown /absolute/path/to/document.md
```

Or, from a Rex pane, open a split without taking focus:

```sh
rex split -b "$REX_BLOCK" --split=right --focus=false --shell=none -- \
  "$(rexdown --check)" -- /absolute/path/to/document.md
```

This standalone command creates a new pane each time; duplicate-preview reuse
belongs to the Amp tool. Neither command depends on the checkout staying in place.

## Controls and rendering

- **Ctrl+P**: open/close the searchable command palette. Type to filter, use
  up/down to select, then Enter (or click) to run a command. Escape dismisses it.
- Mouse wheel / up and down arrows / Page Up / Page Down: scroll
- `j` / `k`: scroll down / up; `g` / `G`: top / bottom
- `d` / `u`: down / up half a screen, like `less` (adapts to pane height)
- Left/right arrows or `h` / `l`: pan horizontally across wide code and tables
- `t`: expand/collapse the clickable table of contents (**hidden by default**)
- Mouse wheel over contents: scroll the heading list
- `r`: refresh; `q` or Ctrl-C: exit (closes a shell-free Rex preview)
- File read failures show a retrying status and preserve the last good content.
- Links within the document navigate to headings. Other links are displayed,
  not opened; previewing a document never runs code or opens URL handlers.

The palette includes **Theme**, **Quit**, **Keys**, **Screenshot**, **Refresh**, and
**Contents**. The default **Terminal** theme inherits the terminal's foreground,
background, and ANSI palette, like Helix's `term*` themes. It does not read or
change terminal configuration. Theme also offers 12 bundled choices: Dark, Light,
Dracula, Nord, Catppuccin Mocha/Latte, Gruvbox Dark/Light, Solarized Dark/Light, TwoDark, and
Monokai Extended. The named themes use the same bundled palette for the UI and
code highlighting; no downloads or system configuration are needed. The choice
lasts for this viewer session. Keys opens scrollable keybinding help
(arrows, Page Up/Down, or mouse wheel; Escape closes it).

Screenshot saves the **visible document viewport**, without the palette overlay,
as a self-contained SVG in **Pictures/Rexdown** (normally `~/Pictures/Rexdown`).
It uses the system Pictures directory when available, otherwise `~/Pictures`.
To choose a different destination for the viewer session:

```sh
rexdown --screenshot-dir ~/Desktop/exports discussion.md
```

The directory is created on export. Relative paths are relative to the viewer's
working directory. Names include the document stem and UTC date/time, for example
`discussion-2026-10-19_14-30-00Z.svg`. A numeric suffix is added on collision;
existing files are never overwritten. The footer reports the saved path or a
write error. Existing screenshots are not moved. It does not upload anything;
the SVG contains the visible document text, so treat it like the original file
when sharing. The document itself is never modified.
In Terminal mode, export queries the terminal's current foreground, background,
and the ANSI colors used by the viewport, then embeds their RGB values in the
SVG. This works in Rex and other terminals supporting OSC 4/10/11 color queries.
If those queries are unavailable, export reports an error; choose a bundled
theme instead. Bundled themes need no terminal queries.

The original UTF-8 Markdown file is the only source of truth. Rexdown polls its
contents every **750 ms**, including same-size edits and editor atomic saves;
it does not use inotify or a platform file watcher. Refresh preserves
the vertical scroll offset, clamped if the document shrinks; it does not track
the same paragraph when text is inserted above it. Headings, tables, highlighted
code, nested lists, task lists, and fenced text diagrams render. Prose and table
cells wrap to the pane; fenced code remains unwrapped. Horizontal panning applies
to the document viewport. Custom HTML/CSS, images, and Mermaid rendering are not
supported; HTML and image descriptions are inert text. Syntax grammars are
bundled in the binary, and unknown fence languages render as plain text.

Files stay on the executor's machine; the plugin does not upload documents or
connect a cloud orb to your Mac. Previewing a local document does not fetch its
linked pages or images.

## Update or uninstall

Close preview panes with `q`, update the checkout, and rerun
`cargo install --path . --locked`. For the copied Amp adapter, move its installed
directory somewhere outside `~/.config/amp/plugins/` as a backup, repeat the
adapter install command, and reload plugins. Neither installation updates
automatically when the checkout changes. Keep the backup until verified.

**Migrating from the Python prototype:** install the Rust binary and replace the
old adapter as above. The old `viewer.py` and uv environment are no longer used.
The tool name, path argument, and pane reuse behavior stay the same.

To uninstall, close the previews, remove only
`~/.config/amp/plugins/rex-markdown/`, and reload plugins or restart Amp. If you
used the development loader, remove only `~/.config/amp/plugins/rex-markdown.ts`
instead. Run `cargo uninstall rexdown` to remove the binary. Neither method
removes your Markdown documents or checkout.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Amp cannot find `rex_open_markdown` | Reload plugins; ensure the installed directory contains `index.ts`. Ask Amp to search its tools for `rex_open_markdown`. |
| `rex` or `rexdown` not found | Run `command -v rex rexdown` in the terminal used to start Amp. Ensure `~/.cargo/bin` is on PATH; restart Amp after fixing it. |
| No Rex connection | Launch Rex and check `rex whoami`. The plugin does not start the server automatically. |
| “Run Amp inside Rex” | Start Amp in the intended Rex pane, or supply the full `anchor` block ID. The plugin deliberately avoids guessing which pane to split. |
| Remote server rejected | This adapter needs a local Unix-socket Rex server. It does not transfer files to remote hosts. |
| “Cannot run rexdown” | Install the binary and run `rexdown --check` from Amp's terminal. No split is created until that check succeeds. There are no launch-time downloads. |
| “Cannot read file · retrying” | Restore the file at the watched path and check permissions/UTF-8 encoding. The last good render stays visible. |
| Viewer code changed but pane looks unchanged | Close/reopen the pane. Reload Amp plugins too if the adapter changed. |

## Develop from a checkout

For local Amp development, put a loader in
`~/.config/amp/plugins/rex-markdown.ts`, using the actual absolute checkout path:

```ts
import register from '/absolute/path/to/rexdown/rex-markdown/index.ts'
export const description = 'Open local Markdown in a live Rex pane.'
export default register
```

Reload that plugin in Amp. This keeps the adapter checkout as the source of truth
and does not publish it. The adapter still launches the installed binary: rerun
`cargo install --path . --locked` and close/reopen previews after Rust changes.
For standalone iteration, use `cargo run -- path/to/document.md`. Reload Amp after
adapter changes. Document edits need neither reload nor reopen.

Run checks from the repository root:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

The tests use Ratatui's headless terminal and real temporary files. They cover
refresh/recovery, scroll clamping, link hit-testing, navigation, ToC toggling,
resizing, Unicode wrapping, tables, highlighting, and diagrams.

GitHub Actions is configured to run these checks and an installation smoke test
on Ubuntu and macOS. This does not test the real Rex server or
Amp host. Before shipping integration changes, run the install/preview smoke
check above in Rex, including reopening the same file and verifying that focus
stays in Amp.

## License

[BSD 3-Clause](LICENSE), copyright 2026 Trey Aspelund. Retain the license when
redistributing the viewer or plugin.

Bundled themes come from [two-face](https://docs.rs/two-face/), curated by the
bat project. See [theme library notices](THEME_LICENSES.md); run
`rexdown --theme-licenses` for the embedded asset license texts.
