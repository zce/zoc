#!/usr/bin/env python3
"""Print the deterministic SHA-256 of a built frontend tree."""
import hashlib
import pathlib
import sys


def main() -> None:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "frontend")
    digest = hashlib.sha256()
    files = sorted(path for path in root.rglob("*") if path.is_file())
    for path in files:
        digest.update(str(path.relative_to(root)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
    print(digest.hexdigest())


if __name__ == "__main__":
    main()
