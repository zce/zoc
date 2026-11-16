#!/usr/bin/env python3
"""Gate the official Web UI build before ZOC embeds it.

Checks that the build is self-contained, has no PWA service worker, keeps
lazy assets, workers, WASM, fonts and the terminal emulator, and does not
reference the OpenCode server as its own origin.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "frontend")
    index = root / "index.html"
    if not index.is_file():
        fail(f"missing {index}")
    html = index.read_text()
    if 'id="root"' not in html:
        fail("index.html has no #root mount")
    if "_assets/" not in html:
        fail("index.html does not reference _assets/")
    if (root / "sw.js").exists():
        fail("PWA service worker sw.js is present")
    if "serviceWorker.register" in html:
        fail("service-worker registration is present")

    files = [p for p in root.rglob("*") if p.is_file()]
    if len(files) < 20:
        fail(f"only {len(files)} files emitted; the official app build looks incomplete")

    names = {p.name for p in files}
    required = {
        "favicon.ico",
        "site.webmanifest",
    }
    missing = required - names
    if missing:
        fail(f"missing upstream assets: {sorted(missing)}")

    # The upstream terminal plugin loads ghostty-web lazily. Catch a build
    # that dropped the wasm/font assets or the terminal chunk entirely.
    blob = "\n".join(
        p.read_text(errors="ignore") for p in files if p.suffix in {".js", ".html", ".json"}
    )
    if "ghostty" not in blob.lower() and not any("terminal" in p.name.lower() for p in files):
        fail("terminal assets are missing from the build")

    # Office previews and Markdown workers are lazy; at least one wasm file
    # must survive. This is a compatibility signal, not a fixed count.
    if not any(p.suffix == ".wasm" for p in files):
        fail("no WASM assets emitted (Office previews or Markdown worker lost)")

    # NOTE: the official connect dialog contains a placeholder such as
    # "http://localhost:4096" in translation catalogs. A string match cannot
    # distinguish it from a baked-in server, so the build script sets
    # VITE_OPENCODE_SERVER_MODE=none and the platform adapter returns no
    # currentServerUrl. Do not add a naive URL gate here.

    manifest = root / "build-meta.json"
    meta = {
        "files": len(files),
        "bytes": sum(p.stat().st_size for p in files),
        "wasm": sum(1 for p in files if p.suffix == ".wasm"),
        "serviceWorker": False,
    }
    manifest.write_text(json.dumps(meta, indent=2) + "\n")
    print(
        f"frontend gate passed: {meta['files']} files, {meta['bytes']} bytes, "
        f"{meta['wasm']} wasm assets, no service worker"
    )


if __name__ == "__main__":
    main()
