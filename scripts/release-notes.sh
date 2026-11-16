#!/usr/bin/env bash
# Generate Markdown release notes for a ZOC upstream sync.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${ZOC_VERSION:?ZOC_VERSION is required}"
UPSTREAM_SHA="${UPSTREAM_SHA:?UPSTREAM_SHA is required}"
PREVIOUS_SHA="${PREVIOUS_SHA:-}"
PREVIOUS_TAG="${PREVIOUS_TAG:-}"
META="${ROOT}/build/linux-amd64/upstream.json"

package_version="$(jq -r '.upstreamPackageVersion' "${META}")"
mygo_version="$(jq -r '.mygoVersion' "${META}")"

cat <<EOF
# ZOC ${VERSION}

## OpenCode upstream

- Repository: \`anomalyco/opencode\`
- Branch: \`dev\`
- Commit: \`${UPSTREAM_SHA}\`
- Web app version: \`${package_version}\`
- MyGo: \`${mygo_version}\`

EOF

if [[ -n "${PREVIOUS_SHA}" ]]; then
  echo "## Upstream changes"
  echo
  response="$(curl -fsSL -H "Accept: application/vnd.github+json" \
    -H "Authorization: Bearer ${GH_TOKEN:-}" \
    "https://api.github.com/repos/anomalyco/opencode/compare/${PREVIOUS_SHA}...${UPSTREAM_SHA}" || true)"
  if [[ -n "${response}" ]]; then
    total="$(jq -r '.total_commits // 0' <<<"${response}")"
    echo "${total} commits since \`${PREVIOUS_SHA}\`."
    echo
    jq -r '.commits[:20][] | "- `" + (.sha[0:7]) + "` " + (.commit.message | split("\n")[0])' <<<"${response}" || true
    if [[ "${total}" -gt 20 ]]; then
      echo "- …and $((total - 20)) more"
    fi
  else
    echo "Upstream compare data was unavailable; see the commit above."
  fi
  echo
else
  echo "## Upstream changes"
  echo
  echo "First published ZOC sync from this upstream line."
  echo
fi

if [[ -n "${PREVIOUS_TAG}" ]]; then
  zoc_changes="$(git log --oneline --no-decorate "${PREVIOUS_TAG}..HEAD" | head -30)"
else
  zoc_changes="$(git log --oneline --no-decorate -20)"
fi

cat <<EOF
## ZOC changes

${zoc_changes}

## Artifacts

- Fedora/RPM and a user-managed archive are attached.
- \`SHA256SUMS\` covers every artifact and \`upstream.json\`.
- \`upstream.json\` records the exact upstream SHA, toolchain versions, adapter hashes and build image.

RPM installs are updated by DNF. The user-managed archive is installed by \`install.sh\`; signed in-app updates are available only when the updater key pair is provisioned.
EOF
