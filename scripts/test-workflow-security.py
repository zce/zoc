#!/usr/bin/env python3
"""Static security regression checks for the upstream release workflow."""
from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / ".github/workflows/upstream-sync.yml"
SIGNED = ROOT / "scripts/build-signed.sh"


def job_sections(text: str) -> dict[str, str]:
    sections: dict[str, list[str]] = {}
    current = ""
    for line in text.splitlines():
        match = re.match(r"^  ([a-z][a-z0-9_-]*):\s*$", line)
        if match:
            current = match.group(1)
            sections[current] = [line]
        elif current:
            sections[current].append(line)
    return {name: "\n".join(lines) for name, lines in sections.items()}


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
    if "MYGO_UPDATER_PRIVATE_KEY" not in sign:
        errors.append("sign job does not receive the updater private key")
    policy = verify.find("Check signing continuity")
    build = verify.find("Run gates")
    if policy < 0 or build < 0:
        errors.append("verify job does not have the expected policy/build steps")
    else:
        for match in re.finditer("MYGO_UPDATER_PRIVATE_KEY", verify):
            if not (policy < match.start() < build):
                errors.append("verify job exposes the key outside the policy step")
    if "scripts/build-signed.sh" not in sign:
        errors.append("sign job does not use the isolated signing script")
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
    if "scripts/build-frontend.sh" in sign:
        errors.append("sign job runs the upstream frontend build")

    signed = SIGNED.read_text()
    if "-skip-build-command" not in signed:
        errors.append("signing script does not skip the upstream build command")
    if "build-frontend.sh" in signed:
        errors.append("signing script invokes the upstream frontend build")
    if "MYGO_UPDATER_PRIVATE_KEY" not in signed:
        errors.append("signing script does not use the supported MyGo signing key")
    if "prepare-upstream.sh" not in signed:
        errors.append("signing script does not pin the upstream checkout")

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1
    print("workflow credential separation checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
