# Live Markdown in Rex

Standalone terminal Markdown viewer with an Amp plugin adapter, developed in
its own jj repository. The `rex-markdown/` directory is an Amp directory plugin;
publishing/installing the adapter is separate from running the viewer locally.

Ask Amp: **“Open this Markdown file in a Rex pane.”** The `rex_open_markdown`
tool, once installed, opens a split to the right of Amp without taking keyboard
focus. Reopening the same file from the same thread and Rex window reuses the
running preview.

Requires `rex` and `uv` on PATH and a local Rex server. Run Amp inside Rex, or
provide an explicit `anchor` block ID. This does not connect cloud orbs to your
Mac or transfer documents between machines. Textual is installed into uv's
isolated cache on first use; subsequent launches reuse it.

The original UTF-8 Markdown file is the only source of truth. Edits refresh
within about a second, preserving the vertical scroll offset (clamped if the
document shrinks). This is a terminal renderer, not the browser HTML design:
headings, tables, highlighted code, and text diagrams render; custom HTML/CSS,
images, and Mermaid rendering are not supported.

- Mouse wheel / arrow keys / Page Up / Page Down: scroll
- `j` / `k`: scroll down / up; `g` / `G`: top / bottom
- `t`: expand/collapse the clickable table of contents (**hidden by default**)
- `r`: refresh; `q`: exit and close the preview pane
- File read failures show a retrying status and preserve the last good content.
- Links within the document navigate to headings. Other links are displayed,
  not opened; previewing a document never runs code or opens URL handlers.

Standalone use:

```sh
uv run --script rex-markdown/viewer.py /absolute/path/to/document.md
```

For local Amp development, put a loader in
`~/.config/amp/plugins/rex-markdown.ts`, using the actual absolute checkout path:

```ts
import register from '/absolute/path/to/amp-rex-markdown/rex-markdown/index.ts'
export const description = 'Open local Markdown in a live Rex pane.'
export default register
```

Reload that plugin in Amp. This keeps the checkout as the source of truth and
does not publish it. Reload after adapter changes; close/reopen existing panes
after viewer-code changes. Document edits need neither reload nor reopen.

Tests:

```sh
uv run --with textual==8.2.8 python -m unittest discover -s rex-markdown -p 'test_*.py'
```
