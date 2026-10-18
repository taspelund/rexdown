import tempfile
import unittest
from pathlib import Path

from textual.widgets import MarkdownViewer, Static
from viewer import LiveMarkdown


class LiveMarkdownTests(unittest.IsolatedAsyncioTestCase):
    async def test_live_edits_preserve_scroll_and_recover_from_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "board.md"
            source = "# Original\n\n" + "\n\n".join(f"Paragraph {i}" for i in range(60))
            path.write_text(source, encoding="utf-8")
            app = LiveMarkdown(path)
            async with app.run_test(size=(90, 24)) as pilot:
                viewer = app.query_one(MarkdownViewer)
                await pilot.pause()
                await pilot.press("pagedown", "j", "j")
                await pilot.pause()
                offset = viewer.scroll_y
                self.assertGreater(offset, 10)

                # Same-length edit defeats a watcher that only compares file size.
                changed = source.replace("Original", "Replaced")
                path.write_text(changed, encoding="utf-8")
                await pilot.pause(1.1)
                self.assertEqual(viewer.document.source, changed)
                self.assertEqual(viewer.scroll_y, offset)

                path.unlink()
                await pilot.pause(1.1)
                self.assertIn(
                    "Cannot read file", str(app.query_one("#status", Static).content)
                )
                self.assertEqual(viewer.document.source, changed)

                path.write_text("# Short replacement\n\nRecovered", encoding="utf-8")
                await pilot.pause(1.1)
                self.assertEqual(
                    viewer.document.source, "# Short replacement\n\nRecovered"
                )
                self.assertEqual(viewer.scroll_y, 0)
                self.assertNotIn(
                    "Cannot read file", str(app.query_one("#status", Static).content)
                )

    async def test_navigation_and_narrow_resize(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "board.md"
            path.write_text(
                "# Board\n\n[External](https://example.com)\n\n"
                "[Other file](missing.md)\n\n[Jump](#candidate)\n\n"
                + "Paragraph\n\n" * 50
                + "## Candidate\n\nNot accepted",
                encoding="utf-8",
            )
            app = LiveMarkdown(path)
            async with app.run_test(size=(110, 32)) as pilot:
                viewer = app.query_one(MarkdownViewer)
                await pilot.pause()
                self.assertFalse(viewer.show_table_of_contents)
                # Header and footer leave 30 document rows: half a screen is 15.
                await pilot.press("d")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, 15)
                await pilot.press("u", "u")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, 0)
                paragraphs = list(app.query("MarkdownParagraph"))
                # Click real links: external and file links must not crash or
                # replace the watched document; anchors still navigate.
                for paragraph in paragraphs[:2]:
                    await pilot.click(paragraph, offset=(2, 0))
                    await pilot.pause()
                    self.assertEqual(viewer.document.source, path.read_text())
                await pilot.click(paragraphs[2], offset=(2, 0))
                await pilot.pause()
                self.assertGreater(viewer.scroll_y, 40)
                await pilot.press("G")
                await pilot.pause()
                self.assertGreater(viewer.scroll_y, 40)
                await pilot.press("g", "t")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, 0)
                self.assertTrue(viewer.show_table_of_contents)
                await pilot.press("t")
                await pilot.resize_terminal(45, 20)
                await pilot.pause()
                self.assertFalse(viewer.show_table_of_contents)
                # Resizing leaves 18 document rows; do not retain the old step.
                await pilot.press("d")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, 9)
                await pilot.press("G")
                await pilot.pause()
                bottom = viewer.scroll_y
                await pilot.press("d")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, bottom)
                await pilot.press("u")
                await pilot.pause()
                self.assertEqual(viewer.scroll_y, bottom - 9)
                self.assertEqual(
                    viewer.document.source, path.read_text(encoding="utf-8")
                )
                await pilot.press("q")
                self.assertFalse(app.is_running)
