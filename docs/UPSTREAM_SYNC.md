# Upstream sync and releases

`.github/workflows/upstream-sync.yml` is the automation. It runs daily and on manual dispatch.

## Behavior

1. Resolve `anomalyco/opencode` `dev` to a full 40-character SHA, or use the optional `sha` input (recovery/rollback retry).
2. Find the newest published `zoc-v<semver>` release and read its `upstream.json`.
3. If the upstream SHA is unchanged, exit successfully without building or releasing.
4. Compute the next ZOC patch version, independent of OpenCode's package version, and verify the tag is unused.
5. Check the signing continuity policy against the previous stable release.
6. In the unprivileged `verify` job, build the pinned image, run `scripts/check.sh`, build **unsigned** artifacts, and run the WebKitGTK + Native UI smoke test. This job has no signing key or write token.
7. Hand the verified static frontend (identified by SHA-256), the actual upstream package identity, and the hash-pinned MyGo CLI binary to the isolated `sign` job. It builds a trusted tree from `git archive HEAD` and runs ZOC's Go/MyGo build with `-skip-build-command`; it does not checkout or install upstream code, so no untrusted build or install script executes with the updater key present.
8. In the `publish` job, verify checksums and channel signing state, smoke-test the exact artifact, generate notes, then tag and publish. This job has a write token but no signing key.

Any failure before the final publish publishes nothing. The previous release remains installable.

Manual dispatch has a `publish` boolean. Set it to `false` for a full dry run that builds and gates but does not tag or publish. Set `force` to `true` only for a deliberate ZOC patch release that must ship while the upstream SHA is unchanged; otherwise an unchanged upstream exits successfully without a release.

## Build pinning

`ZOC_UPSTREAM_COMMIT` overrides the SHA in `upstream/upstream.json` for a build. The script validates the 40-character SHA, fetches exactly that commit, checks it out, cleans the generated checkout, applies patches with `git apply --check`, and records the SHA in `build/linux-amd64/upstream.json`. The credential-free build reads `packages/app/package.json` from that exact checkout, not the repository baseline; the signing stage receives the package name and version with the verified frontend. The metadata test in the build record changes the package version and confirms it propagates.

`upstream/upstream.json` is the local/repository baseline used by developer builds. The automation does not push to `main`; each release's attached `upstream.json` is the authoritative record of what was built. Update the repository baseline in a reviewed PR when adopting a new SHA for local builds.

## Credential separation

The `verify` job holds no signing secret and no write-capable GitHub token. It performs all upstream checkout, dependency installation, Vite build, ZOC gates, unsigned packaging and smoke. It hands the verified frontend archive, the actual upstream package identity, and the pinned MyGo CLI binary to the signing job.

The `sign` job does not checkout or execute upstream code and does not install dependencies. It builds a fresh signing tree from `git archive HEAD` of the reviewed commit, extracts the verified frontend into it, copies the pinned MyGo CLI only after checking its SHA-256 against `scripts/mygo-cli.env`, and runs `scripts/build-signed.sh` through that tree's `container.sh`. The signing container therefore contains trusted ZOC source, the verified frontend, the hash-pinned MyGo binary, and the signing key; there is no untrusted preparation process or shared process environment. `build-signed.sh` re-checks the frontend hash and the MyGo digest, skips the upstream `buildCommand`, and invokes MyGo's supported signing path. The package name and version are passed from the credential-free verify job; the pinned upstream SHA remains the source identity.

The `publish` job receives the signed or unsigned archive, not the signing key, verifies checksums, runs the final smoke on that artifact, and only then creates the tag through the scoped GitHub API and publishes. Checkouts use `persist-credentials: false`, so the smoke-tested binary never runs beside a write-capable git credential.

`deltas: 0` is deliberate: delta generation would make the signing job query GitHub release history, and full signed archives keep that job free of a GitHub credential. The release still carries a signed update archive and manifest.

## Signing continuity

`scripts/signing-policy.py` checks the previous stable release before any build:

- no previous release + no keys: unsigned release allowed;
- both keys: first signed release allowed;
- previous signed release: both keys required, and the public key must match `updaterPublicKey` from the previous `upstream.json`;
- previous signed release with missing/incomplete metadata: fail closed;
- a manifest asset without matching signed metadata: fail closed.

The workflow downloads the previous `upstream.json`, detects a previous update manifest, and fails before building when the policy is inconsistent. Unit tests cover initial unsigned, first signed, continued signed, missing key, changed key, partial keys and inconsistent metadata.

## Release metadata

Every release includes `upstream.json` with at least:

- repository, branch, full commit SHA;
- ZOC SemVer;
- MyGo, Go and Bun versions;
- adapter patch identifiers and SHA-256 hashes;
- adapter source hashes;
- actual `packages/app/package.json` version captured by the credential-free build from the checked-out upstream commit;
- frontend tree SHA-256;
- build image identifier;
- build timestamp;
- update signing state and public updater key when signed;
- `serviceWorker: false`.

`SHA256SUMS` covers every attached file.

## Recovery

- Re-run the workflow with the last known-good full SHA. The version increments; no tag is overwritten.
- If a bad release was published, mark it as a prerelease or edit its notes, then dispatch a known-good SHA. Do not delete or force-push tags.
- To retry a failed build, re-run the same workflow run; no tag exists yet, so the same version is chosen.

## Updater keys (one-time, explicit owner action)

Signed in-app updates require a key pair. The owner must generate it in a Podman container and provision secrets; this automation does not create keys.

```sh
podman run --rm -it -v "$PWD:/src:z" -w /src \
  localhost/zoc-build:2026-11-16 \
  bash -lc 'bun x mygo-cli keygen -o /tmp/zoc-keys && cat /tmp/zoc-keys/mygo-update.pub'
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
- CI `43925109520` on `f09b641`: passed with the real-desktop acceptance evidence and native-inbox screenshot.
- CI `43999777415` on `a6a1f83`: passed with the credential-isolated workflow, storage/pagination tests and release-security checks.
- Final CI `44000865794` on `5cbb280`: passed after the scoped-tag and credential-persistence hardening.
- Credential-isolation dry run `44000865900` on `5cbb280` (`force=true`, `publish=false`): the unprivileged verify job built and smoked the unsigned artifact, the signing job was skipped because the channel has no keys, and publishing was skipped. No release was created.
- Release run `44093223394` on `242cef9` (`force=true`, `publish=true`, upstream `2cb1818d01bb8bbfdbe37f2b64436fce6328fc6a`): verify and publish passed, signing was skipped because the channel has no keys, and `zoc-v0.1.3` was published with the Linux desktop shell, all three adapter patches and six adapter source hashes in `upstream.json`.
- The downloaded `zoc-v0.1.1` assets passed `sha256sum -c SHA256SUMS`; `install.sh` installed the archive into a temporary `HOME`, and the installed binary passed the WebKitGTK + Native UI smoke.
- `scripts/container.sh bash -lc 'sh build/linux-amd64/install.sh build/linux-amd64/zoc-0.1.0-linux-amd64.tar.gz'` installs into a temporary `HOME`; the installed `~/.local/bin/zoc` is executable and the desktop entry is written.

## Rebuilding a published release

The release workflow passes `ZOC_VERSION` (and `ZOC_UPSTREAM_COMMIT` for the tested SHA) into the packaging build. To rebuild an existing release:

```sh
git fetch --tags
git checkout zoc-v0.1.1
export ZOC_VERSION=0.1.1
export ZOC_UPSTREAM_COMMIT=e4606463e7d18fd126ecfac3ec04602f1e44814b
./scripts/container.sh ./scripts/check.sh
./scripts/container.sh ./scripts/build.sh
```

The values are in the release's attached `upstream.json`. The repository `VERSION`/`upstream/upstream.json` files are the baseline for local builds and may be one patch ahead of an older tag after a forced ZOC-only release; use the tag's release metadata rather than guessing. Two consecutive local builds at the same tree produced identical `zoc` and RPM hashes and an identical frontend hash.

Runs before `43999786405` used the earlier single-job workflow. They remain historical evidence but do not attest to the new credential boundary.

## Refinement tests (2026-11-17)

- `python3 scripts/test-signing-policy.py`: 8 cases covering initial unsigned, first signed, continued signed, missing/changed/partial keys and inconsistent previous metadata.
- `python3 scripts/test-workflow-security.py`: asserts the private key is confined to the isolated signing step, the sign job never runs upstream checkout or dependency installation, the signing tree comes from `git archive HEAD`, the frontend and MyGo CLI are hash-verified, and write permission exists only in publishing.
- `bun test adapter`: 5 storage cases covering failed set/remove/clear, ordered writes after a rejection, and initial-load retry.
- `go test ./internal/opencode ./internal/desktop ./internal/native`: cursor pagination, snapshot cursor propagation, load-more merging, an empty filtered first page that still offers **Load more sessions** and reveals a later match, repeated-refresh ordering, stale errors and server-switch invalidation.
- Local signed build with a throwaway key from a trusted `git archive HEAD` tree: the sentinel `prepare-upstream.sh` and `build-frontend.sh` scripts were not executed, the pinned MyGo digest and frontend hash were verified, `update-linux-amd64.json` was produced, and metadata/signature checks and the WebKitGTK + Native UI smoke passed.
- The unsigned build produces `updateSigning: not-provisioned` and no manifest; checksums verify.
