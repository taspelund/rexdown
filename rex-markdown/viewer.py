# /// script
# requires-python = ">=3.11"
# dependencies = ["textual==8.2.8"]
# ///
"""Read-only, live Markdown preview. Run with: uv run --script viewer.py FILE."""

import argparse
import asyncio
import sys
from datetime import datetime
from pathlib import Path
from typing import ClassVar

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.widgets import Footer, MarkdownViewer, Static


class ReadOnlyViewer(MarkdownViewer):
    async def go(self, location: str) -> None:
        # MarkdownViewer handles link events before they reach the app. Override
        # its navigation boundary so links cannot replace the watched document.
        if location.startswith("#"):
            self.document.goto_anchor(location[1:])
        else:
            self.notify(location, title="Link (preview is read-only)", markup=False)


class LiveMarkdown(App):
    TITLE = "Live Markdown"
    CSS = """
    #status { height: 1; padding: 0 1; background: $primary-background; }
    #status.error { color: $error; }
    MarkdownViewer { padding: 0 1; }
    MarkdownTableOfContents { width: 30; }
    """
    BINDINGS: ClassVar[list[Binding]] = [
        Binding("q", "quit", "Close", priority=True),
        Binding("t", "contents", "Contents", priority=True),
        Binding("r", "reload", "Refresh", priority=True),
        Binding("j", "down", "Down", show=False),
        Binding("k", "up", "Up", show=False),
        Binding("d", "half_page(1)", "Half page down", show=False),
        Binding("u", "half_page(-1)", "Half page up", show=False),
        Binding("g", "top", "Top", show=False),
        Binding("G", "bottom", "Bottom", show=False),
    ]

    def __init__(self, path: Path):
        super().__init__()
        self.path = path
        self.source: str | None = None
        self.updated = "Waiting for file"

    def compose(self) -> ComposeResult:
        yield Static(self.path.name, id="status", markup=False)
        yield ReadOnlyViewer(show_table_of_contents=False, open_links=False)
        yield Footer()

    async def on_mount(self) -> None:
        viewer = self.query_one(MarkdownViewer)
        viewer.focus()
        await self.reload_file()
        # Cancel refresh (including an in-flight read) before the viewer unmounts.
        viewer.set_interval(0.75, self.reload_file)

    async def reload_file(self) -> None:
        if not self.is_running:
            return
        status = self.query_one("#status", Static)
        try:
            source = await asyncio.to_thread(self.path.read_text, encoding="utf-8")
        except (OSError, UnicodeError) as error:
            status.add_class("error")
            status.update(f"Cannot read file · retrying · {self.path.name}: {error}")
            return  # Keep the last good render while editors replace the file.
        status.remove_class("error")
        if source != self.source:
            viewer = self.query_one(MarkdownViewer)
            offset = viewer.scroll_offset
            await viewer.document.update(source)
            # Layout changes after children mount. Restore after layout, not before it.
            self.call_after_refresh(viewer.scroll_to, offset.x, offset.y, animate=False)
            self.source = source
            self.updated = f"Updated {datetime.now():%H:%M:%S}"
        status.update(f"{self.path.name} · LIVE · {self.updated}")

    async def action_reload(self) -> None:
        await self.reload_file()

    def action_contents(self) -> None:
        viewer = self.query_one(MarkdownViewer)
        viewer.show_table_of_contents = not viewer.show_table_of_contents

    def action_down(self) -> None:
        self.query_one(MarkdownViewer).scroll_down(animate=False)

    def action_up(self) -> None:
        self.query_one(MarkdownViewer).scroll_up(animate=False)

    def action_half_page(self, direction: int) -> None:
        viewer = self.query_one(MarkdownViewer)
        rows = max(1, viewer.scrollable_content_region.height // 2)
        viewer.scroll_relative(y=direction * rows, animate=False)

    def action_top(self) -> None:
        self.query_one(MarkdownViewer).scroll_home(animate=False)

    def action_bottom(self) -> None:
        self.query_one(MarkdownViewer).scroll_end(animate=False)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path, nargs="?")
    parser.add_argument(
        "--check",
        action="store_true",
        help="Print the prepared Python interpreter and exit",
    )
    args = parser.parse_args()
    if args.check:
        print(sys.executable)
    elif args.path:
        LiveMarkdown(args.path.absolute()).run()
    else:
        parser.error("a Markdown path is required")
