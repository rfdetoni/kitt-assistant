import unittest
from pathlib import Path

import kitt.remote


class RemoteAssetsTests(unittest.TestCase):
    def test_frontend_surface_renderer_is_declarative(self):
        root = Path(kitt.remote.__file__).resolve().parent / "static"
        js = (root / "app.js").read_text(encoding="utf-8")
        surface = (root / "surface.js").read_text(encoding="utf-8")
        self.assertIn('from "./surface.js"', js)
        self.assertIn("class SurfaceRenderer", surface)
        self.assertIn("/api/surfaces/", surface)
        self.assertNotIn(".innerHTML", surface)

    def test_frontend_uses_sse_not_polling(self):
        root = Path(kitt.remote.__file__).resolve().parent / "static"
        js = (root / "app.js").read_text(encoding="utf-8")
        html = (root / "index.html").read_text(encoding="utf-8")
        self.assertIn("new EventSource", js)
        self.assertNotIn("setInterval(", js)
        self.assertIn('src="/app.js"', html)
        self.assertNotIn("<script>", html)


if __name__ == "__main__":
    unittest.main()
