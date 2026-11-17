#!/usr/bin/env python3
"""Check that a file is a PNG and print its dimensions."""
import struct
import sys


def main() -> None:
    path = sys.argv[1]
    with open(path, "rb") as f:
        header = f.read(33)
    if header[:8] != bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]):
        raise SystemExit(f"{path}: not a PNG")
    width, height = struct.unpack(">II", header[16:24])
    print(f"PNG {width}x{height}: {path}")


if __name__ == "__main__":
    main()
