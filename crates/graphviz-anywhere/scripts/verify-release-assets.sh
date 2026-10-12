#!/usr/bin/env bash
#
# Verify that every release asset the published crate can ask for already exists
# on the release the crate will point at.
#
# build.rs downloads
#
#     ${GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL}/v${CARGO_PKG_VERSION}/${asset}
#
# so the *crate version* — not the git tag — selects the release. Two ways that
# goes wrong, both of which break `cargo install` for every downstream crate and
# neither of which can be undone (a crates.io version is immutable):
#
#   1. The crate is published before the assets are attached to the release.
#      `needs: [release]` only proves the release job ran; the packaging steps
#      use `|| true` and action-gh-release does not fail on unmatched files, so
#      an asset can go missing without failing the build.
#   2. The tag and packages/rust/Cargo.toml disagree, so the published crate
#      points at some other release's assets.
#
# Run this between "create the release" and "cargo publish".
#
# Usage:
#   ./scripts/verify-release-assets.sh [--tag <git tag>]
#
# The tag defaults to $GITHUB_REF_NAME. A `v*` tag must agree with the crate
# version (checked here) and then names the same release build.rs would reach
# for. Any other tag — e.g. the `test*` rehearsal tags, which exist precisely to
# validate the asset set before the real tag is cut — skips the version
# cross-check and checks that tag's own release instead. With no tag at all the
# release is `v${crate version}`, i.e. exactly what the published crate encodes.
#
# Environment variables:
#   GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL - same override build.rs honours

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TAG="${GITHUB_REF_NAME:-}"

while [[ $# -gt 0 ]]; do
    case $1 in
        --tag) TAG="$2"; shift 2 ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

check_command curl

MANIFEST="${PROJECT_ROOT}/packages/rust/Cargo.toml"
HELPERS="${PROJECT_ROOT}/packages/rust/src/build_helpers.rs"

# The crate version is what build.rs interpolates into the download URL.
CRATE_VERSION="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "${MANIFEST}" | head -n 1)"
if [[ -z "${CRATE_VERSION}" ]]; then
    log_error "cannot read the package version from ${MANIFEST}"
    exit 1
fi

# Which release to check. Default to the one the published crate will encode.
RELEASE="v${CRATE_VERSION}"

if [[ -n "${TAG}" && "${TAG}" == v* ]]; then
    TAG_VERSION="${TAG#v}"
    if [[ "${TAG_VERSION}" != "${CRATE_VERSION}" ]]; then
        log_error "tag ${TAG} disagrees with the crate version ${CRATE_VERSION}"
        log_error "the published crate would download .../v${CRATE_VERSION}/<asset>, i.e. a"
        log_error "different release than the one being built. Bump packages/rust/Cargo.toml"
        log_error "to ${TAG_VERSION} (or tag v${CRATE_VERSION}) before publishing."
        exit 1
    fi
    log_info "tag ${TAG} matches the crate version"
elif [[ -n "${TAG}" ]]; then
    log_info "tag ${TAG} is not a v* release tag; checking that release, and"
    log_info "skipping the crate-version cross-check"
    RELEASE="${TAG}"
fi

# Source of truth: the mapping build.rs actually consults. Read only the
# function body so the unit-tests below it cannot contribute stray names.
ASSETS=()
while IFS= read -r asset; do
    ASSETS+=("${asset}")
done < <(
    awk '/^pub fn target_triple_to_asset_name/ { inside = 1 }
         inside { print }
         inside && /^}/ { exit }' "${HELPERS}" |
        grep -oE 'graphviz-native-[A-Za-z0-9._-]+\.(tar\.gz|zip)' |
        sort -u
)

if [[ ${#ASSETS[@]} -eq 0 ]]; then
    log_error "no asset names found in ${HELPERS} — the extraction is broken, not the release"
    exit 1
fi

BASE_URL="${GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL:-https://github.com/Actrium/supramark/releases/download}"
log_info "checking ${#ASSETS[@]} assets against ${BASE_URL}/${RELEASE}"

MISSING=()
for asset in "${ASSETS[@]}"; do
    url="${BASE_URL}/${RELEASE}/${asset}"
    # A one-byte range GET rather than HEAD: it exercises the same redirect to
    # the asset CDN that build.rs's `curl -L` will follow, and a draft release
    # answers 404 here exactly as it would for a downstream user.
    code="$(curl -sL --retry 3 --retry-delay 2 -r 0-0 -o /dev/null -w '%{http_code}' "${url}" || echo 000)"
    case "${code}" in
        200|206) log_info "  present  ${asset}" ;;
        *)
            log_error "  MISSING  ${asset} (HTTP ${code}) ${url}"
            MISSING+=("${asset}")
            ;;
    esac
done

if [[ ${#MISSING[@]} -gt 0 ]]; then
    log_error "${#MISSING[@]} of ${#ASSETS[@]} assets are not on release ${RELEASE} — refusing to publish"
    exit 1
fi

log_info "all ${#ASSETS[@]} assets are present on release ${RELEASE}"
