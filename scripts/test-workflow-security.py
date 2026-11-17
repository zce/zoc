#!/usr/bin/env python3
"""Static security regression checks for the upstream release workflow."""
from __future__ import annotations

import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / ".github/workflows/upstream-sync.yml"
SIGNED = ROOT / "scripts/build-signed.sh"
METADATA = ROOT / "scripts/write-release-metadata.sh"
MYGO_ENV = ROOT / "scripts/mygo-cli.env"


def job_sections(text: str) -> dict[str, str]:
    sections: dict[str, list[str]] = {}
    current = ""
    for line in text.splitlines(keepends=True):
        match = re.match(r"^  ([a-z][a-z0-9_-]*):\s*$", line)
        if match:
            current = match.group(1)
            sections[current] = [line]
        elif current:
            sections[current].append(line)
    return {name: "".join(lines) for name, lines in sections.items()}


def step_blocks(job: str) -> dict[str, str]:
    offsets: list[tuple[str, int]] = []
    offset = 0
    for line in job.splitlines(keepends=True):
        match = re.match(r"^      - name: (.+?)\s*$", line)
        if match:
            offsets.append((match.group(1), offset))
        offset += len(line)
    blocks: dict[str, str] = {}
    for index, (name, start) in enumerate(offsets):
        end = offsets[index + 1][1] if index + 1 < len(offsets) else len(job)
        blocks[name] = job[start:end]
    return blocks


def main() -> int:
    text = WORKFLOW.read_text()
    jobs = job_sections(text)
    errors: list[str] = []

    for name in ("verify", "sign", "publish"):
        if name not in jobs:
            errors.append(f"missing job {name}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1

    verify = jobs["verify"]
    sign = jobs["sign"]
    publish = jobs["publish"]

    if "MYGO_UPDATER_PRIVATE_KEY" in publish:
        errors.append("publish job has access to the updater private key")
    if "contents: write" in verify or "contents: write" in sign:
        errors.append("build or signing job has a write token")
    if "contents: write" not in publish:
        errors.append("publish job lacks repository write permission")
    if text.count("persist-credentials: false") != 3:
        errors.append("checkouts retain a GitHub credential")
    if "git push" in publish:
        errors.append("publish job pushes with a persisted git credential")
    if "gh api" not in publish:
        errors.append("publish job does not create the tag through the scoped token")

    continuity = step_blocks(verify).get("Check signing continuity", "")
    presence_only = "HAS_PRIVATE_KEY: ${{ secrets.MYGO_UPDATER_PRIVATE_KEY != '' }}"
    if not continuity or "Run gates" not in verify:
        errors.append("verify job does not have the expected policy/build steps")
    if verify.count(presence_only) != 1 or presence_only not in continuity:
        errors.append("verify must derive only a boolean key-presence flag")
    if "MYGO_UPDATER_PRIVATE_KEY" in verify.replace(presence_only, "", 1):
        errors.append("verify job exposes or references the private key beyond presence checking")
    if '--has-private-key "${HAS_PRIVATE_KEY}"' not in continuity:
        errors.append("signing policy must consume only the boolean key-presence flag")

    for name in ("package", "package_version", "frontend_sha"):
        if f"{name}:" not in verify.split("steps:", 1)[0]:
            errors.append(f"verify job does not expose {name}")

    if "zoc-signing-toolchain" not in verify:
        errors.append("verify job does not upload the pinned signing toolchain")
    if "Upload pinned signing toolchain" not in verify:
        errors.append("verify job is missing the signing toolchain upload step")
    if "scripts/mygo-cli.env" not in verify or "sha256sum -c -" not in verify:
        errors.append("verify job does not hash-check the pinned signing toolchain")
    if "build/zoc-mygo" not in verify:
        errors.append("verify job does not stage the pinned MyGo CLI")

    sign_steps = step_blocks(sign)
    signing_steps = [name for name, block in sign_steps.items() if "MYGO_UPDATER_PRIVATE_KEY" in block]
    if signing_steps != ["Build and sign without untrusted preparation"]:
        errors.append(f"signing key is exposed outside the isolated signing step: {signing_steps}")
    if "MYGO_UPDATER_PRIVATE_KEY" not in sign:
        errors.append("sign job does not receive the updater private key")

    for forbidden in ("bun install", "bun run build", "prepare-upstream.sh", "build-frontend.sh", "env -u"):
        if forbidden in sign:
            errors.append(f"sign job still runs untrusted preparation: {forbidden}")
    if "zoc-signing-toolchain" not in sign:
        errors.append("sign job does not download the pinned signing toolchain")
    if "git archive HEAD" not in sign:
        errors.append("sign job does not build a trusted signing tree from the commit")
    if "/tmp/zoc-sign/scripts/container.sh" not in sign:
        errors.append("signing stage does not use the trusted signing tree")
    if "sha256sum -c -" not in sign:
        errors.append("sign job does not verify the pinned MyGo CLI digest")
    if "unsupported entry types" not in sign:
        errors.append("sign job accepts unsafe frontend archive entries")

    signed = SIGNED.read_text()
    for forbidden in ("prepare-upstream.sh", "bun install", "bun run build", "build-frontend.sh", "env -u"):
        if forbidden in signed:
            errors.append(f"signing script still runs untrusted preparation: {forbidden}")
    for required in ('"${ROOT}/mygo" build -skip-build-command', "scripts/mygo-cli.env", "sha256sum", "ZOC_UPSTREAM_PACKAGE"):
        if required not in signed:
            errors.append(f"signing script is missing: {required}")

    metadata = METADATA.read_text()
    if "ZOC_UPSTREAM_PACKAGE" not in metadata or "ZOC_UPSTREAM_PACKAGE_VERSION" not in metadata:
        errors.append("release metadata does not accept verified package identity")
    if "ACTUAL_COMMIT" in metadata and "checked-out upstream" not in metadata:
        errors.append("release metadata lost the checked-out upstream guard")

    mygo_env = dict(re.findall(r'^([A-Z0-9_]+)="([^"]*)"', MYGO_ENV.read_text(), re.MULTILINE))
    upstream = json.loads((ROOT / "upstream/upstream.json").read_text())
    if mygo_env.get("MYGO_CLI_VERSION") != upstream.get("mygoVersion"):
        errors.append("pinned MyGo CLI version differs from upstream/upstream.json")
    if not re.fullmatch(r"[0-9a-f]{64}", mygo_env.get("MYGO_CLI_LINUX_X64_SHA256", "")):
        errors.append("pinned MyGo CLI digest is missing")

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1
    print("workflow credential separation checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
