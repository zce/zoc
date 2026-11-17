#!/usr/bin/env python3
"""Enforce the release channel's update-signing continuity.

The previous ZOC release's upstream.json is the channel state. Once a stable
release was signed, every later release must be signed with the same public
key. Before the first signed release, an unsigned release is allowed.

This script is intentionally policy-only: it does not handle keys or produce
signatures.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path


def load_previous(path: str) -> dict | None:
    if not path or not Path(path).is_file():
        return None
    with open(path) as f:
        return json.load(f)


def evaluate(
    previous: dict | None,
    previous_has_manifest: bool,
    public_key: str,
    has_private_key: bool,
) -> bool:
    public_key = (public_key or "").strip()
    previous_signed = bool(previous and previous.get("updateSigning") == "provisioned")
    previous_key = ((previous or {}).get("updaterPublicKey") or "").strip()

    # An earlier signed manifest is channel state even if its metadata was
    # incomplete; fail closed rather than silently downgrade it.
    if previous_has_manifest:
        previous_signed = True

    if previous and previous_key and not previous_signed:
        raise ValueError("previous release records an updater key but is not marked signed")
    if previous_signed:
        if not previous_key:
            raise ValueError("previous release has signed updates but no updaterPublicKey metadata")
        if not public_key:
            raise ValueError("this channel is signed; ZOC_UPDATER_PUBLIC_KEY is required")
        if not has_private_key:
            raise ValueError("this channel is signed; MYGO_UPDATER_PRIVATE_KEY is required")
        if public_key != previous_key:
            raise ValueError("the updater public key differs from the previous signed release")
        return True

    if public_key and not has_private_key:
        raise ValueError("ZOC_UPDATER_PUBLIC_KEY is set but MYGO_UPDATER_PRIVATE_KEY is missing")
    if has_private_key and not public_key:
        raise ValueError("MYGO_UPDATER_PRIVATE_KEY is set but ZOC_UPDATER_PUBLIC_KEY is missing")
    return bool(public_key and has_private_key)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--previous", default="", help="previous release upstream.json")
    parser.add_argument("--previous-has-manifest", default="false")
    parser.add_argument("--public-key", default="")
    parser.add_argument("--has-private-key", default="false")
    parser.add_argument("--out", default="", help="GITHUB_OUTPUT file")
    args = parser.parse_args()

    try:
        signed = evaluate(
            load_previous(args.previous),
            args.previous_has_manifest == "true",
            args.public_key,
            args.has_private_key == "true",
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    value = "true" if signed else "false"
    if args.out:
        with open(args.out, "a") as out:
            out.write(f"signed={value}\n")
    else:
        print(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
