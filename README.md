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

## Requirements

- [Rex](https://www.superlogical.com/rex) running locally, with `rex` on PATH.
- [uv](https://docs.astral.sh/uv/getting-started/installation/) on PATH. On macOS
  with Homebrew: `brew install uv`.
- [Amp](https://ampcode.com/docs), only if you want the agent-operated tool.
- A copy of this repository. Git and jj both work; jj is not required to use it.

Run commands below from the repository root, in a Rex terminal. Check the tools:

```sh
rex whoami
uv --version
amp --version  # optional for standalone use
```

The viewer uses Python 3.11+ and pins Textual to 8.2.8. `uv` prepares an isolated
environment automatically; no global `pip install`, Node, Bun, or build step is
needed. The first launch needs network access to download dependencies (and
Python if needed); later launches reuse uv's cache. Amp supplies its own plugin
runtime.

Tested on macOS with Rex CLI `0.0.1792158932-g88fd41`. Other platforms and older Rex
versions have not been validated. The plugin requires a local Unix-socket Rex
connection and the CLI's `split`, `block inspect`, `block ls`, and `block call
process` commands.

## Install the Amp plugin on this machine

Copy the complete `rex-markdown/` directory, not just `index.ts`. This installs
the plugin for your local Amp sessions across projects, without relying on the
checkout remaining in place. The guard avoids overwriting an existing install
or the development loader described below.

```sh
destination="$HOME/.config/amp/plugins/rex-markdown"
if [ -e "$destination" ] || [ -L "$destination" ] ||
   [ -e "$destination.ts" ] || [ -L "$destination.ts" ]; then
  printf 'Already installed at %s (or .ts); see Update below.\n' "$destination"
else
  mkdir -p "$(dirname "$destination")" &&
    cp -R ./rex-markdown "$destination" &&
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
uv run --script rex-markdown/viewer.py /absolute/path/to/document.md
```

Or, from a Rex pane, open a split without taking focus:

```sh
rex split -b "$REX_BLOCK" --split=right --focus=false --shell=none -- \
  uv run --script "$PWD/rex-markdown/viewer.py" /absolute/path/to/document.md
```

This standalone command creates a new pane each time; duplicate-preview reuse
belongs to the Amp tool. Keep the checkout in place when using these commands.

## Controls and rendering

- Mouse wheel / arrow keys / Page Up / Page Down: scroll
- `j` / `k`: scroll down / up; `g` / `G`: top / bottom
- `d` / `u`: down / up half a screen, like `less` (adapts to pane height)
- `t`: expand/collapse the clickable table of contents (**hidden by default**)
- `r`: refresh; `q`: exit and close the preview pane
- File read failures show a retrying status and preserve the last good content.
- Links within the document navigate to headings. Other links are displayed,
  not opened; previewing a document never runs code or opens URL handlers.

The original UTF-8 Markdown file is the only source of truth. Refresh preserves
the vertical scroll offset, clamped if the document shrinks; it does not track
the same paragraph when text is inserted above it. Headings, tables, highlighted
code, and text diagrams render. Wide code blocks scroll horizontally. Custom
HTML/CSS, images, and Mermaid rendering are not supported.

Files stay on the executor's machine; the plugin does not upload documents or
connect a cloud orb to your Mac. Previewing a local document does not fetch its
linked pages or images.

## Update or uninstall

For a copy-based install, first close its preview panes with `q`. Move the
installed directory somewhere outside `~/.config/amp/plugins/` as a backup, then
repeat the install command from the updated checkout and reload plugins. The
copy does not update automatically when you update the checkout. Keep the old
copy until you have verified the new one.

To uninstall, close the previews, remove only
`~/.config/amp/plugins/rex-markdown/`, and reload plugins or restart Amp. If you
used the development loader, remove only `~/.config/amp/plugins/rex-markdown.ts`
instead. Neither method removes your Markdown documents or checkout.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Amp cannot find `rex_open_markdown` | Reload plugins; ensure the installed directory contains both `index.ts` and `viewer.py`. Ask Amp to search its tools for `rex_open_markdown`. |
| `rex` or `uv` not found | Run `command -v rex uv` in the terminal used to start Amp. Restart Amp after fixing PATH. |
| No Rex connection | Launch Rex and check `rex whoami`. The plugin does not start the server automatically. |
| “Run Amp inside Rex” | Start Amp in the intended Rex pane, or supply the full `anchor` block ID. The plugin deliberately avoids guessing which pane to split. |
| Remote server rejected | This adapter needs a local Unix-socket Rex server. It does not transfer files to remote hosts. |
| First launch fails installing dependencies | Check network/proxy access and run the dependency check below. No split is created until that check succeeds. |
| “Cannot read file · retrying” | Restore the file at the watched path and check permissions/UTF-8 encoding. The last good render stays visible. |
| Viewer code changed but pane looks unchanged | Close/reopen the pane. Reload Amp plugins too if the adapter changed. |

Dependency check, from the checkout:

```sh
uv run --script rex-markdown/viewer.py --check
```

Success prints the prepared Python interpreter path. For a copied installation,
use `$HOME/.config/amp/plugins/rex-markdown/viewer.py` instead.

## Develop from a checkout

For local Amp development, put a loader in
`~/.config/amp/plugins/rex-markdown.ts`, using the actual absolute checkout path:

```ts
import register from '/absolute/path/to/rexdown/rex-markdown/index.ts'
export const description = 'Open local Markdown in a live Rex pane.'
export default register
```

Reload that plugin in Amp. This keeps the checkout as the source of truth and
does not publish it. Reload after adapter changes; close/reopen existing panes
after viewer-code changes. Document edits need neither reload nor reopen.

Run tests from the repository root:

```sh
uv run --with textual==8.2.8 python -m unittest discover -s rex-markdown -p 'test_*.py'
```

The tests cover live refresh, scroll preservation and clamping, file recovery,
link handling, keyboard navigation, ToC toggling, resizing, and quitting.

Lint and format checks use the repository's `ruff.toml`:

```sh
uvx ruff@0.17.1 check rex-markdown
uvx ruff@0.17.1 format --check rex-markdown
```

GitHub Actions runs these checks and the headless viewer tests on Ubuntu with
Python 3.11 and macOS with Python 3.14. This does not test the real Rex server or
Amp host. Before shipping integration changes, run the install/preview smoke
check above in Rex, including reopening the same file and verifying that focus
stays in Amp.

## License

[BSD 3-Clause](LICENSE), copyright 2026 Trey Aspelund. Retain the license when
redistributing the viewer or plugin.
