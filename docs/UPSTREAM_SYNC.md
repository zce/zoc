# Upstream sync and releases

`.github/workflows/upstream-sync.yml` is the automation. It runs daily and on manual dispatch.

## Behavior

1. Resolve `anomalyco/opencode` `dev` to a full 40-character SHA, or use the optional `sha` input (recovery/rollback retry).
2. Find the newest published `zoc-v<semver>` release and read its `upstream.json`.
3. If the upstream SHA is unchanged, exit successfully without building or releasing.
4. Compute the next ZOC patch version, independent of OpenCode's package version, and verify the tag is unused.
5. Build the pinned image, run `scripts/check.sh`, build artifacts, and run the WebKitGTK smoke test.
6. Generate release notes from the upstream compare and ZOC history.
7. Verify signed update metadata only if both keys are provisioned.
8. Tag `zoc-v<version>` at the tested commit, push the tag, publish the release with artifacts and `upstream.json`.

Any failure before step 8 publishes nothing. The previous release remains installable.

Manual dispatch has a `publish` boolean. Set it to `false` for a full dry run that builds and gates but does not tag or publish. Set `force` to `true` only for a deliberate ZOC patch release that must ship while the upstream SHA is unchanged; otherwise an unchanged upstream exits successfully without a release.

## Build pinning

`ZOC_UPSTREAM_COMMIT` overrides the SHA in `upstream/upstream.json` for a build. The script validates the 40-character SHA, fetches exactly that commit, checks it out, cleans the generated checkout, applies patches with `git apply --check`, and records the SHA in `build/linux-amd64/upstream.json`.

`upstream/upstream.json` is the local/repository baseline used by developer builds. The automation does not push to `main`; each release's attached `upstream.json` is the authoritative record of what was built. Update the repository baseline in a reviewed PR when adopting a new SHA for local builds.

## Release metadata

Every release includes `upstream.json` with at least:

- repository, branch, full commit SHA;
- upstream package name and version;
- ZOC SemVer;
- MyGo, Go and Bun versions;
- adapter patch identifiers and SHA-256 hashes;
- adapter source hashes;
- frontend tree SHA-256;
- build image identifier;
- build timestamp;
- update signing state;
- `serviceWorker: false`.

`SHA256SUMS` covers every attached file.

## Recovery

- Re-run the workflow with the last known-good full SHA. The version increments; no tag is overwritten.
- If a bad release was published, mark it as a prerelease or edit its notes, then dispatch a known-good SHA. Do not delete or force-push tags.
- To retry a failed build, re-run the same workflow run; no tag exists yet, so the same version is chosen.

## Updater keys (one-time, explicit owner action)

Signed in-app updates require a key pair. The owner must generate it in a Podman container and provision secrets; this automation does not create keys.

```sh
podman run --rm -it -v "$PWD:/src:Z" -w /src \
  localhost/zoc-build:2026-11-16 \
  bash -lc 'go run github.com/egoist/mygo/cmd/mygo keygen -o /tmp/zoc-keys && cat /tmp/zoc-keys/mygo-update.pub'
```

Then:

1. Save the printed public key as the repository variable `ZOC_UPDATER_PUBLIC_KEY`.
2. Add the private key contents as the secret `MYGO_UPDATER_PRIVATE_KEY`.
3. Never commit either file; `.gitignore` excludes `mygo-update.key` and `mygo-update.pub`.
4. Confirm a dry-run release contains `update-linux-amd64.json` before claiming signed updates.

Without these, releases are plain RPM/archive artifacts and the app does not claim in-app signed updates.

## Distribution

- RPM: updated by DNF; ZOC's self-updater does not apply.
- Archive + `install.sh`: can use MyGo's signed updater when provisioned.
- Checksums and dependency information are attached to every release.
- Tags use `zoc-v*` and are never force-pushed.

## Verification runs

- Dry run `43910157889` on `mygo`, SHA `e4606463e7d18fd126ecfac3ec04602f1e44814b`, `publish=false`: resolved the SHA, compared releases, built the pinned image, ran `check.sh`, built the Linux artifact, passed the WebKitGTK smoke, generated notes, verified the updater state, and skipped publishing.
- CI `43910154855` on `mygo`: Podman verification, Linux packaging, WebKitGTK smoke, and artifact upload passed.
- CI `43911629510` on `97455cb`: the same checks plus the Native UI screenshot gate passed; the uploaded artifact checksums verified locally.
- Publish run `43917676504`: built `zoc-v0.1.1`, published the release at `https://github.com/zce/zoc/releases/tag/zoc-v0.1.1`, and attached the RPM, archive, installer, `SHA256SUMS` and `upstream.json`.
- Unchanged run `43918630325`: found the same upstream SHA and exited successfully with every build/release step skipped.
- CI `43919001926` on `c07f05e`: passed with the final release workflow and checksum fix.
- The downloaded `zoc-v0.1.1` assets passed `sha256sum -c SHA256SUMS`; `install.sh` installed the archive into a temporary `HOME`, and the installed binary passed the WebKitGTK + Native UI smoke.
- `scripts/container.sh bash -lc 'sh build/linux-amd64/install.sh build/linux-amd64/zoc-0.1.0-linux-amd64.tar.gz'` installs into a temporary `HOME`; the installed `~/.local/bin/zoc` is executable and the desktop entry is written.
