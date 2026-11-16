#!/usr/bin/env python3
"""Print the next ZOC patch version."""
import sys

parts = [int(part) for part in sys.argv[1].split(".")]
if len(parts) != 3:
    raise SystemExit("expected MAJOR.MINOR.PATCH")
parts[2] += 1
print(".".join(str(part) for part in parts))
