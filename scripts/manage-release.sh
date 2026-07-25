#!/usr/bin/env bash
#
# manage-release.sh — drive a monorepo product through its release steps.
#
# Usage: ./scripts/manage-release.sh <subcommand> <project>
#
# See usage() for the subcommands. The <project> is one of the monorepo's
# releasable products; its version is read from that product's canonical
# manifest (Cargo.toml for the Rust products, package.json for the SDK) and its
# git tag / GitHub release is <project>/v<version> (the slash namespaces each
# product's tags; releases before July 2026 used <project>_v<version>).

# mapfile, inherit_errexit, and safe empty-array expansion under `set -u` all
# need bash >= 4.4; macOS's /bin/bash is 3.2, so fail fast before half-running.
if [ -z "${BASH_VERSINFO:-}" ] || [ "${BASH_VERSINFO[0]}" -lt 4 ] \
    || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -lt 4 ]; }; then
    >&2 echo "manage-release.sh requires bash >= 4.4 (macOS: brew install bash)"
    exit 1
fi

set -euo pipefail
# Without this, a failure inside $(release_notes) is silently swallowed and the
# release is created with broken notes.
shopt -s inherit_errexit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO="Start9Labs/start-technologies"
# Registries are scoped per project (<PROJECT>_SOURCE/TARGET_REGISTRY); the OS
# and StartWRT promote source -> target. The OS chain: CI indexes images into
# alpha; alpha -> beta is promoted manually, out of band; the full `release`
# promotes the source (beta) -> target (production). Override either per run.
STARTOS_SOURCE_REGISTRY="${STARTOS_SOURCE_REGISTRY:-https://beta-registry.start9.com}"
STARTOS_TARGET_REGISTRY="${STARTOS_TARGET_REGISTRY:-https://registry.start9.com}"
S3_BUCKET="s3://startos-images"
S3_CDN="https://startos-images.nyc3.cdn.digitaloceanspaces.com"
START9_GPG_KEY="2D63C217"
SDK_NPM_PACKAGE="@start9labs/start-sdk"
# The first heading release_notes() emits. cmd_create_gh_release splits an
# existing release body on it to keep hand-written notes above it, so the two
# must agree — hence one constant rather than the string in both places.
NOTES_MARKER="## What's Changed"

APT_BASE_URL="https://start9-debs.nyc3.digitaloceanspaces.com"
APT_SUITE="stable"
APT_COMPONENT="main"

# StartWRT publishes flashable images to its own registry pair + S3 bucket. The
# chain mirrors the OS, minus the alpha tier: the CI deploy job (start-wrt.yaml)
# uploads a build's images to S3 AND registers + indexes them into the source
# (beta) registry, where beta routers (UCI `startwrt.system.registry` pointed at
# it) soak the version; the full `release` promotes source -> target
# (production). `register` is the manual fallback for CI's register/index steps
# (same commands; keep them in sync — see root AGENTS.md "Coupled changes"). It
# ships two gzipped images: an sdcard image (fresh install -> the registry `img`
# slot) and a sysupgrade image (OTA update -> the `squashfs` slot; see
# cmd_register for the hardlink trick that maps the .img.gz names onto those
# slots). Override either registry per run.
STARTWRT_SOURCE_REGISTRY="${STARTWRT_SOURCE_REGISTRY:-https://startwrt-beta-registry.start9.com}"
STARTWRT_TARGET_REGISTRY="${STARTWRT_TARGET_REGISTRY:-https://startwrt-registry.start9.com}"
STARTWRT_S3_BUCKET="s3://startwrt-images"
STARTWRT_S3_CDN="https://startwrt-images.nyc3.cdn.digitaloceanspaces.com"
STARTWRT_PLATFORM="spacemit,k1-x"
# The CI artifact (start-wrt.yaml `image` job) holding both images.
STARTWRT_BUILD_ARTIFACT="startwrt-openwrt-image"
# Compat floor for `registry os version add`: the oldest installed version
# allowed to upgrade to this one. An explicit beta floor (not a `^` caret) keeps
# beta prerelease tags in range. The upper bound (<=$VERSION) is added in
# cmd_register. Mirrored in start-wrt.yaml's register step.
STARTWRT_COMPAT_FLOOR="${STARTWRT_COMPAT_FLOOR:->=0.1.0-beta.1}"

# Every OS image platform. Most ship an iso + squashfs; raspberrypi ships a
# flashable img + squashfs (no iso). See os_image_exts.
OS_PLATFORMS="x86_64 x86_64-nonfree x86_64-nvidia aarch64 aarch64-nonfree aarch64-nvidia raspberrypi riscv64 riscv64-nonfree"
CLI_TRIPLES="x86_64-unknown-linux-musl x86_64-apple-darwin aarch64-unknown-linux-musl aarch64-apple-darwin riscv64gc-unknown-linux-musl"
DEB_ARCHES="x86_64 aarch64 riscv64"

PROJECTS="start-os start-cli start-tunnel start-registry start-sdk start-wrt"

# --- Project metadata ---

project_kind() {
    case "$1" in
        start-os) echo os ;;
        start-cli) echo cli ;;
        start-tunnel | start-registry) echo deb ;;
        start-sdk) echo npm ;;
        start-wrt) echo wrt ;;
        *) return 1 ;;
    esac
}

derive_version() {
    local project=$1 version
    if [ "$(project_kind "$project")" = npm ]; then
        jq -r .version "$REPO_ROOT/projects/$project/package.json"
        return
    fi
    # StartOS versions carry a revision segment (0.4.0.1) that SemVer, and so Cargo, cannot
    # express; root package.json holds it and projects/start-os/Cargo.toml carries only a
    # `-rev.N` label (kept honest by cmd_pre_check). Mirrors build/env/version.sh.
    if [ "$(project_kind "$project")" = os ]; then
        jq -r .version "$REPO_ROOT/package.json"
        return
    fi
    # start-wrt has no top-level crate; its canonical version lives in the ctrl
    # crate manifest (mirrors the top CHANGELOG.md entry and start-wrt.yaml's
    # "Determine version" step).
    local toml="$REPO_ROOT/projects/$project/Cargo.toml"
    if [ "$project" = start-wrt ]; then
        toml="$REPO_ROOT/projects/start-wrt/backend/ctrl/Cargo.toml"
    fi
    version=$(grep -m1 'VERSION_BUMP' "$toml" 2>/dev/null | sed -E 's/.*version *= *"([^"]+)".*/\1/')
    if [ -z "$version" ]; then
        version=$(sed -nE '/^\[package\]/,/^\[/{s/^version *= *"([^"]+)".*/\1/p}' "$toml" | head -1)
    fi
    echo "$version"
}

changelog_path() { echo "$REPO_ROOT/projects/$1/CHANGELOG.md"; }

cli_asset_name() {
    case "$1" in
        x86_64-unknown-linux-musl) echo x86_64-linux ;;
        aarch64-unknown-linux-musl) echo aarch64-linux ;;
        riscv64gc-unknown-linux-musl) echo riscv64-linux ;;
        x86_64-apple-darwin) echo x86_64-macos ;;
        aarch64-apple-darwin) echo aarch64-macos ;;
        *) return 1 ;;
    esac
}

deb_arch() {
    case "$1" in
        x86_64) echo amd64 ;;
        aarch64) echo arm64 ;;
        riscv64) echo riscv64 ;;
        *) return 1 ;;
    esac
}

os_platform_label() {
    case "$1" in
        x86_64-nonfree) echo "x86_64/AMD64" ;;
        x86_64-nvidia) echo "x86_64/AMD64 + NVIDIA" ;;
        x86_64) echo "x86_64/AMD64-slim (FOSS-only)" ;;
        aarch64-nonfree) echo "aarch64/ARM64" ;;
        aarch64-nvidia) echo "aarch64/ARM64 + NVIDIA" ;;
        aarch64) echo "aarch64/ARM64-slim (FOSS-only)" ;;
        raspberrypi) echo "Raspberry Pi (aarch64)" ;;
        riscv64-nonfree) echo "RISCV64 (RVA23)" ;;
        riscv64) echo "RISCV64 (RVA23)-slim (FOSS-only)" ;;
        *) echo "$1" ;;
    esac
}

# The image extensions a platform ships: squashfs everywhere, plus iso (most) or
# a flashable img (raspberrypi).
os_image_exts() {
    case "$1" in
        raspberrypi) echo "squashfs img" ;;
        *) echo "squashfs iso" ;;
    esac
}

# --- Helpers ---

# Load a registry's OS index into $_INDEX_JSON, refetching only when asked for a
# different registry than the one held — a release reads a dozen asset URLs off
# it and each fetch is a round trip. Call it once at the top of a scope that
# loops over assets: asset_url runs inside $(...), so a fetch it does itself is
# discarded with that subshell, while one the caller did is inherited.
_INDEX_REGISTRY=""
_INDEX_JSON=""
load_registry_index() {
    if [ "$1" != "$_INDEX_REGISTRY" ]; then
        _INDEX_JSON=$(start-cli --registry="$1" registry os index 2>/dev/null || echo '{}')
        _INDEX_REGISTRY=$1
    fi
}

# The published URL of an indexed asset: <registry> <slot> <platform>. Empty if
# that registry carries no such asset for $VERSION.
#
# This — never a URL rebuilt from the bucket layout plus a local filename — is
# what a download link must point at. The published basename carries the build's
# commit hash (startos-0.4.0-514af0c_x86_64.iso), which nothing local
# reproduces, so a reconstructed URL 403s.
asset_url() {
    load_registry_index "$1"
    jq -r --arg v "$VERSION" --arg s "$2" --arg p "$3" \
        '.versions[$v][$s][$p].urls[0] // empty' <<< "$_INDEX_JSON"
}

# The signed blake3 commitment of an indexed asset, as hex (b3sum's output
# format): <registry> <slot> <platform>. Empty if the asset is not indexed.
asset_commitment_b3() {
    load_registry_index "$1"
    local b64
    b64=$(jq -r --arg v "$VERSION" --arg s "$2" --arg p "$3" \
        '.versions[$v][$s][$p].commitment.hash // empty' <<< "$_INDEX_JSON")
    [ -n "$b64" ] || return 0
    # The index stores the hash unpadded-base64; base64 -d wants the padding.
    case $((${#b64} % 4)) in
        2) b64="${b64}==" ;;
        3) b64="${b64}=" ;;
    esac
    printf '%s' "$b64" | base64 -d | od -An -v -tx1 | tr -d ' \n'
}

parse_run_id() {
    local val="$1"
    if [[ "$val" =~ /actions/runs/([0-9]+) ]]; then
        echo "${BASH_REMATCH[1]}"
    else
        echo "$val"
    fi
}

# The commit the tag will point at ($COMMIT, default HEAD), as a full sha.
tag_commit_sha() { (cd "$REPO_ROOT" && git rev-parse --verify "${COMMIT:-HEAD}^{commit}"); }

# Local staging dir: keeps the flat _v separator (the tag's / would nest dirs).
release_dir() { echo "$HOME/Downloads/${PROJECT}_v${VERSION}"; }

# Marker recording which commit the release dir's GHA artifacts were built from.
gha_commit_file() { echo "$(release_dir)/.gha-commit"; }

ensure_release_dir() {
    local dir
    dir=$(release_dir)
    if [ "${CLEAN:-}" = "1" ]; then
        rm -rf "$dir"
    fi
    mkdir -p "$dir"
    cd "$dir"
}

enter_release_dir() {
    local dir
    dir=$(release_dir)
    if [ ! -d "$dir" ]; then
        >&2 echo "Release directory $dir does not exist. Run 'pull-gha' or 'pull' first."
        exit 1
    fi
    cd "$dir"
}

# List the CLI binaries in the (current) release dir, one per line.
cli_binaries() {
    local f
    for f in start-cli_*; do
        case "$f" in *.asc | *.deb) continue ;; esac
        [ -f "$f" ] && echo "$f"
    done
}

# List the .deb packages in the (current) release dir, one per line.
deb_files() {
    local f
    for f in *.deb; do [ -f "$f" ] && echo "$f"; done
}

# Compress every raw .img in the release dir beside itself.
#
# The registry keeps indexing the image bare: an installer streams it and the
# signed commitment is over the raw bytes. But a raw card image is mostly
# zeroes, and the release notes hand it to a human on a browser — so the notes
# link the .gz, and it is hashed and signed alongside the bare image.
#
# Idempotent: skips a .gz already newer than its source, and stages through a
# temp file so an interrupted run can't leave a truncated one looking current.
# A no-op outside the os kind — nothing else ships a raw .img.
ensure_img_gz() {
    local img
    for img in *.img; do
        [ -f "$img" ] || continue
        if [ -f "${img}.gz" ] && [ ! "$img" -nt "${img}.gz" ]; then
            continue
        fi
        >&2 echo "  compressing ${img} -> ${img}.gz"
        gzip -c "$img" > "${img}.gz.tmp"
        mv -f "${img}.gz.tmp" "${img}.gz"
    done
}

# List every file the project ships (for signing / checksums), one per line.
release_files() {
    local f
    case "$KIND" in
        os) for f in *.iso *.img *.img.gz *.squashfs; do [ -f "$f" ] && echo "$f"; done ;;
        cli) cli_binaries; deb_files ;;
        deb) deb_files ;;
        wrt) for f in *-sdcard.img.gz *-sysupgrade.img.gz; do [ -f "$f" ] && echo "$f"; done ;;
    esac
}

resolve_gh_user() {
    GH_USER=${GH_USER:-$(gh api user -q .login 2>/dev/null || true)}
    GH_GPG_KEY=$(git config user.signingkey 2>/dev/null || true)
}

require_kind() {
    local ok
    for ok in "$@"; do
        [ "$KIND" = "$ok" ] && return 0
    done
    >&2 echo "Subcommand '$SUBCOMMAND' does not apply to $PROJECT (kind: $KIND)."
    exit 2
}

# Print the CHANGELOG body for $VERSION (between its heading and the next `## `).
changelog_section() {
    awk -v v="$VERSION" '
        /^## / {
            if (started) exit
            if (index($0, v) > 0) { started = 1; next }
        }
        started { print }
    ' "$(changelog_path "$PROJECT")"
}

# --- Deb helpers (shared by the deb and cli kinds) ---

# Download this project's per-arch debs from a GitHub Actions run into the cwd.
pull_gha_debs() {
    local arch
    for arch in $DEB_ARCHES; do
        echo "  ${PROJECT}_${arch}.deb"
        gh run download -R "$REPO" "$RUN_ID" -n "${PROJECT}_${arch}.deb" -D "$(pwd)"
    done
}

# Download this project's released debs from the apt repository into the cwd.
pull_apt_debs() {
    local arch darch idx filename
    for arch in $DEB_ARCHES; do
        darch=$(deb_arch "$arch")
        idx="${APT_BASE_URL}/dists/${APT_SUITE}/${APT_COMPONENT}/binary-${darch}/Packages"
        filename=$(curl -fsSL "$idx" 2>/dev/null | awk -v pkg="$PROJECT" -v ver="$VERSION" '
            /^$/ { p=""; v="" }
            /^Package:/ { p=$2 }
            /^Version:/ { v=$2 }
            /^Filename:/ { if (p==pkg && index(v, ver) > 0) print $2 }
        ' | head -1)
        if [ -n "$filename" ]; then
            echo "  ${arch}: ${filename}"
            curl -fsSL "${APT_BASE_URL}/${filename}" -o "$(basename "$filename")"
        else
            >&2 echo "  ! no ${PROJECT} ${arch} deb for ${VERSION} in apt repo"
        fi
    done
}

# Publish the debs in the cwd to the apt repository and the GitHub release.
publish_debs() {
    local files file
    mapfile -t files < <(deb_files)
    if [ ${#files[@]} -eq 0 ]; then
        >&2 echo "No .deb files in $(release_dir)"
        return 1
    fi
    echo "Publishing ${PROJECT} debs to the apt repository..."
    "$REPO_ROOT/debian/publish.sh" "${files[@]}"
    echo "Uploading ${PROJECT} debs to GitHub release ${TAG}..."
    for file in "${files[@]}"; do
        gh release upload -R "$REPO" "$TAG" "$file" --clobber
    done
}

# --- Subcommands ---

# Report a failed "already released" guard. With FORCE=1 it's tolerated (returns
# success) so an idempotent step can be re-run — S3 put -P, gh release --clobber,
# registry re-index, apt re-publish all overwrite in place. Non-idempotent steps
# (npm publish, which can't republish a version) must NOT use this.
release_guard() {
    if [ "${FORCE:-}" = 1 ]; then
        >&2 echo "  ! ${1} (forced)"
        return 0
    fi
    >&2 echo "  ✗ ${1}"
    return 1
}

cmd_pre_check() {
    local errors=0
    echo "Pre-checking ${PROJECT} v${VERSION} (tag ${TAG})..."

    # 1. The TOP changelog heading must be this prospective version explicitly
    #    (never `## [Unreleased]`) — see root AGENTS.md changelog rule. Testing the
    #    first `## ` heading (not the whole file) rejects a stale `## [Unreleased]`
    #    sitting above the version heading, which would also drop its entries from
    #    the generated release notes (changelog_section reads from the heading down).
    local changelog ver_re first_heading
    changelog=$(changelog_path "$PROJECT")
    ver_re=${VERSION//./\\.}
    if [ ! -f "$changelog" ]; then
        >&2 echo "  ✗ no CHANGELOG.md at $changelog"
        errors=1
    else
        first_heading=$(grep -m1 -E '^## ' "$changelog")
        if printf '%s\n' "$first_heading" | grep -qE "^##[[:space:]]+\[?${ver_re}(]| |\$)"; then
            echo "  ✓ top changelog heading is ${VERSION}"
        else
            >&2 echo "  ✗ top CHANGELOG.md heading must be ${VERSION} (found: ${first_heading:-none}); a bare '## [Unreleased]' top heading is not allowed — see root AGENTS.md"
            errors=1
        fi
    fi

    # 1b. StartOS install/update docs pin the GitHub release link to the version
    # being shipped — a repo-wide releases/latest resolves to whichever product
    # released most recently (e.g. StartTunnel), not to StartOS. Enforce the bump
    # like the changelog: fail if any doc still says releases/latest, links to no
    # ${TAG} release, or pins a stale version. See root AGENTS.md "Coupled changes".
    if [ "$KIND" = os ]; then
        local docs_src total good
        docs_src="$REPO_ROOT/projects/start-os/docs/src"
        if grep -rqF "releases/latest" "$docs_src" 2>/dev/null; then
            >&2 echo "  ✗ start-os docs still link to releases/latest — pin to https://github.com/${REPO}/releases/tag/${TAG}"
            errors=1
        fi
        # `|| true`: zero grep matches must reach the ✗ report below, not trip
        # errexit via pipefail on the assignment.
        total=$(grep -rohF "${REPO}/releases/tag/" "$docs_src" 2>/dev/null | wc -l | tr -d ' ' || true)
        good=$(grep -rohF "${REPO}/releases/tag/${TAG}" "$docs_src" 2>/dev/null | wc -l | tr -d ' ' || true)
        if [ "$good" -eq 0 ]; then
            >&2 echo "  ✗ start-os docs link to no ${TAG} release — pin to https://github.com/${REPO}/releases/tag/${TAG}"
            errors=1
        elif [ "$total" -ne "$good" ]; then
            >&2 echo "  ✗ start-os docs pin a stale release link (expected releases/tag/${TAG}):"
            grep -rnF "${REPO}/releases/tag/" "$docs_src" 2>/dev/null | grep -vF "releases/tag/${TAG}" | sed 's/^/      /' >&2
            errors=1
        else
            echo "  ✓ start-os docs pin the release link to ${TAG}"
        fi

        # The crate manifest can't hold the OS version, so it carries a label
        # (0.4.0.1 -> 0.4.0-rev.1). Nothing reads it; a stale one just misleads.
        local expect_label crate_label
        case "$VERSION" in
            *.*.*.*) expect_label="${VERSION%.*}-rev.${VERSION##*.}" ;;
            *) expect_label="$VERSION" ;;
        esac
        crate_label=$(sed -nE '/^\[package\]/,/^\[/{s/^version *= *"([^"]+)".*/\1/p}' \
            "$REPO_ROOT/projects/start-os/Cargo.toml" | head -1)
        if [ "$crate_label" = "$expect_label" ]; then
            echo "  ✓ start-os crate label is ${expect_label}"
        else
            >&2 echo "  ✗ projects/start-os/Cargo.toml version must be ${expect_label} for OS ${VERSION} (found: ${crate_label:-none})"
            errors=1
        fi
    fi

    # 1c. The `s9pk init-package` template pins the SDK version authors scaffold
    # against; hold it at the version being released. The template ships no
    # package-lock.json — `init-package` runs `npm install`, so each scaffold
    # generates its own lock; a committed template lock is dead weight that only
    # rots out of sync with the pin. See start-sdk AGENTS.md and root AGENTS.md
    # "Coupled changes".
    if [ "$PROJECT" = start-sdk ]; then
        local tmpl tmpl_pin
        tmpl="$REPO_ROOT/projects/start-sdk/docs/package-template"
        tmpl_pin=$(jq -r '.dependencies["@start9labs/start-sdk"] // ""' "$tmpl/package.json" 2>/dev/null)
        if [ "$tmpl_pin" != "$VERSION" ]; then
            >&2 echo "  ✗ package-template pins @start9labs/start-sdk@${tmpl_pin:-<none>} — bump to ${VERSION} (make -C projects/start-sdk sync-template)"
            errors=1
        else
            echo "  ✓ package-template pins @start9labs/start-sdk@${VERSION}"
        fi
        if [ -e "$tmpl/package-lock.json" ]; then
            >&2 echo "  ✗ package-template must not commit package-lock.json (init-package regenerates it per scaffold; a committed one only rots out of sync with the SDK pin) — remove it"
            errors=1
        else
            echo "  ✓ package-template ships no package-lock.json"
        fi
    fi

    # 2. Git tag must not already exist on the remote (idempotent: FORCE re-tags).
    if git ls-remote --tags origin "refs/tags/${TAG}" 2>/dev/null | grep -q .; then
        release_guard "tag ${TAG} already exists on origin" || errors=1
    else
        echo "  ✓ tag ${TAG} is free"
    fi

    # 3. This release's own output must not already exist. For os/cli/deb that's
    # the GitHub release (the os images themselves are published to S3 + indexed
    # by CI, so the registry is expected to already carry them). For npm it's the
    # published package version.
    case "$KIND" in
        os | cli | deb | wrt)
            if gh release view -R "$REPO" "$TAG" >/dev/null 2>&1; then
                release_guard "GitHub release ${TAG} already exists" || errors=1
            else
                echo "  ✓ GitHub release ${TAG} does not exist"
            fi
            ;;
        npm)
            # npm can't republish a version, so this is never forceable.
            if [ -n "$(npm view "${SDK_NPM_PACKAGE}@${VERSION}" version 2>/dev/null || true)" ]; then
                >&2 echo "  ✗ ${SDK_NPM_PACKAGE}@${VERSION} already published to npm (cannot republish)"
                errors=1
            else
                echo "  ✓ ${SDK_NPM_PACKAGE}@${VERSION} not yet on npm"
            fi
            ;;
    esac

    # 4. Preconditions for the release steps: everything the pipeline needs must
    # already be in place, so a release doesn't fail halfway through.
    case "$KIND" in
        os)
            # `release` pulls the images from the source registry and promotes
            # them into prod, so every expected asset must already be in source.
            local missing platform ext
            load_registry_index "$STARTOS_SOURCE_REGISTRY"
            if ! jq -e --arg v "$VERSION" '.versions[$v]' <<< "$_INDEX_JSON" >/dev/null 2>&1; then
                >&2 echo "  ✗ OS ${VERSION} not in source registry ${STARTOS_SOURCE_REGISTRY} — promote it there first"
                errors=1
            else
                missing=""
                for platform in $OS_PLATFORMS; do
                    for ext in $(os_image_exts "$platform"); do
                        [ -n "$(asset_url "$STARTOS_SOURCE_REGISTRY" "$ext" "$platform")" ] \
                            || missing="${missing} ${platform}.${ext}"
                    done
                done
                if [ -n "$missing" ]; then
                    >&2 echo "  ✗ source registry is missing OS assets:${missing}"
                    errors=1
                else
                    echo "  ✓ source registry has all ${VERSION} images"
                fi
            fi
            # `release` promotes into prod; it shouldn't already be there.
            if start-cli --registry="$STARTOS_TARGET_REGISTRY" registry os index 2>/dev/null \
                | jq -e ".versions[\"$VERSION\"]" >/dev/null 2>&1; then
                release_guard "OS ${VERSION} already in production registry ${STARTOS_TARGET_REGISTRY}" || errors=1
            else
                echo "  ✓ not yet in production registry"
            fi
            # promoting re-signs registry commitments with the developer key;
            # start-cli reads id.key.pem (auto-migrating a legacy developer.key.pem).
            if [ -f "$HOME/.startos/id.key.pem" ] || [ -f "$HOME/.startos/developer.key.pem" ]; then
                echo "  ✓ developer key present"
            else
                >&2 echo "  ✗ ~/.startos/id.key.pem missing (needed to promote to the registry)"
                errors=1
            fi
            ;;
        npm)
            if npm whoami >/dev/null 2>&1; then
                echo "  ✓ npm authenticated ($(npm whoami 2>/dev/null))"
            else
                >&2 echo "  ✗ not logged in to npm (run: npm login)"
                errors=1
            fi
            ;;
        wrt)
            # `release` pulls the images from the source (beta) registry and
            # promotes them into production, so both assets must already be
            # registered there (the CI deploy does that; `register` is the
            # manual fallback).
            local slot wrt_missing
            load_registry_index "$STARTWRT_SOURCE_REGISTRY"
            if ! jq -e --arg v "$VERSION" '.versions[$v]' <<< "$_INDEX_JSON" >/dev/null 2>&1; then
                >&2 echo "  ✗ StartWRT ${VERSION} not in source registry ${STARTWRT_SOURCE_REGISTRY} — run the start-wrt deploy workflow (or 'register') first"
                errors=1
            else
                wrt_missing=""
                for slot in img squashfs; do
                    [ -n "$(asset_url "$STARTWRT_SOURCE_REGISTRY" "$slot" "$STARTWRT_PLATFORM")" ] \
                        || wrt_missing="${wrt_missing} ${slot}"
                done
                if [ -n "$wrt_missing" ]; then
                    >&2 echo "  ✗ source registry is missing StartWRT assets:${wrt_missing}"
                    errors=1
                else
                    echo "  ✓ source registry has both ${VERSION} images"
                fi
            fi
            # `release` promotes into production; it shouldn't already be there.
            if start-cli --registry="$STARTWRT_TARGET_REGISTRY" registry os index 2>/dev/null \
                | jq -e ".versions[\"$VERSION\"]" >/dev/null 2>&1; then
                release_guard "StartWRT ${VERSION} already in production registry ${STARTWRT_TARGET_REGISTRY}" || errors=1
            else
                echo "  ✓ not yet in production registry"
            fi
            # promoting re-signs registry commitments with the developer key;
            # start-cli reads id.key.pem (auto-migrating a legacy developer.key.pem).
            if [ -f "$HOME/.startos/id.key.pem" ] || [ -f "$HOME/.startos/developer.key.pem" ]; then
                echo "  ✓ developer key present"
            else
                >&2 echo "  ✗ ~/.startos/id.key.pem missing (needed to promote to the registry)"
                errors=1
            fi
            ;;
    esac

    # gh is needed by every release to create the GitHub release (plus the asset
    # upload, sign, and apt Release signature for os/cli/deb/wrt).
    if gh auth status >/dev/null 2>&1; then
        echo "  ✓ gh authenticated"
    else
        >&2 echo "  ✗ gh not authenticated (run: gh auth login)"
        errors=1
    fi
    # os/cli/deb/wrt also sign their artifacts with the Start9 org key, and
    # render checksum blocks into the release notes (see checksum_block).
    if [ "$KIND" != npm ]; then
        if gpg --list-secret-keys "$START9_GPG_KEY" >/dev/null 2>&1; then
            echo "  ✓ Start9 signing key ${START9_GPG_KEY} present"
        else
            >&2 echo "  ✗ Start9 GPG secret key ${START9_GPG_KEY} not in keyring (needed to sign)"
            errors=1
        fi
        if command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1; then
            echo "  ✓ sha-256 tool available"
        else
            >&2 echo "  ✗ neither sha256sum nor shasum installed (needed for release-notes checksums)"
            errors=1
        fi
        if command -v b3sum >/dev/null 2>&1; then
            echo "  ✓ b3sum available"
        else
            >&2 echo "  ✗ b3sum not installed (needed for release-notes checksums; brew/cargo install b3sum)"
            errors=1
        fi
    fi

    # cli/deb publish debs to the apt repo and os pushes the compressed images
    # (push-gz) — all three upload to S3, which needs s3cmd + credentials.
    if [ "$KIND" = cli ] || [ "$KIND" = deb ] || [ "$KIND" = os ]; then
        if command -v s3cmd >/dev/null 2>&1; then
            echo "  ✓ s3cmd available"
        else
            >&2 echo "  ✗ s3cmd not installed (needed to upload to S3/apt)"
            errors=1
        fi
        if [ -f "$HOME/.s3cfg" ] || { [ -n "${S3_ACCESS_KEY:-}" ] && [ -n "${S3_SECRET_KEY:-}" ]; }; then
            echo "  ✓ s3 credentials configured"
        else
            >&2 echo "  ! no ~/.s3cfg and S3_ACCESS_KEY/S3_SECRET_KEY unset — S3 upload may fail"
        fi
    fi

    if [ "$errors" -ne 0 ]; then
        >&2 echo "Pre-check failed."
        exit 1
    fi
    echo "Pre-check passed."
}

cmd_pull_gha() {
    require_kind os cli deb wrt

    if [ -z "${RUN_ID:-}" ]; then
        read -rp "RUN_ID (GitHub Actions run for ${PROJECT}): " RUN_ID
    fi
    RUN_ID=$(parse_run_id "${RUN_ID:-}")
    if [ -z "$RUN_ID" ]; then
        >&2 echo "RUN_ID is required"
        exit 2
    fi

    # The tag must point at the commit these artifacts were built from.
    local run_sha tag_sha
    run_sha=$(gh run view -R "$REPO" "$RUN_ID" --json headSha -q .headSha)
    tag_sha=$(tag_commit_sha)
    if [ "$run_sha" != "$tag_sha" ]; then
        >&2 echo "Run ${RUN_ID} built commit ${run_sha},"
        >&2 echo "but tag ${TAG} would be cut at ${COMMIT:-HEAD} (${tag_sha})."
        >&2 echo "Pass the run for that commit, or set COMMIT=${run_sha}."
        exit 1
    fi

    ensure_release_dir
    echo "$run_sha" > "$(gha_commit_file)"
    echo "Downloading ${PROJECT} artifacts from run ${RUN_ID} (commit ${run_sha})..."

    case "$KIND" in
        os)
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    echo "  ${platform}.${ext}"
                    gh run download -R "$REPO" "$RUN_ID" -n "${platform}.${ext}" -D "$(pwd)"
                done
            done
            ensure_img_gz
            ;;
        cli)
            for triple in $CLI_TRIPLES; do
                local name
                name=$(cli_asset_name "$triple")
                echo "  start-cli_${triple} -> start-cli_${name}"
                gh run download -R "$REPO" "$RUN_ID" -n "start-cli_${triple}" -D "$(pwd)"
                mv start-cli "start-cli_${name}"
            done
            pull_gha_debs
            ;;
        deb)
            pull_gha_debs
            ;;
        wrt)
            echo "  ${STARTWRT_BUILD_ARTIFACT} (sdcard + sysupgrade .img.gz)"
            gh run download -R "$REPO" "$RUN_ID" -n "$STARTWRT_BUILD_ARTIFACT" -D "$(pwd)"
            ;;
    esac
}

cmd_pull() {
    ensure_release_dir
    # Released assets replace any GHA-pulled ones, so the marker no longer applies.
    rm -f "$(gha_commit_file)"
    echo "Downloading released ${PROJECT} v${VERSION} from its official location..."

    case "$KIND" in
        os)
            # start-cli names its output startos-<ver>_<platform>.<ext>, which
            # drops the commit hash the published basename carries. Land each
            # asset under its published name instead, so checksums, signatures,
            # and the .gz key all agree with what is actually on S3.
            local platform ext url published tmp stale expected=()
            load_registry_index "$STARTOS_SOURCE_REGISTRY"
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    url=$(asset_url "$STARTOS_SOURCE_REGISTRY" "$ext" "$platform")
                    if [ -z "$url" ]; then
                        >&2 echo "  (no '$ext' asset indexed for $platform, skipping)"
                        continue
                    fi
                    published=$(basename "$url")
                    echo "  ${ext} ${platform} -> ${published}"
                    tmp=$(mktemp -d "$(pwd)/.get-${platform}-${ext}.XXXXXX")
                    start-cli --registry="$STARTOS_SOURCE_REGISTRY" registry os asset get "$ext" "$VERSION" "$platform" -d "$tmp"
                    mv -f "$tmp"/* "$published"
                    rmdir "$tmp"
                    expected+=("$published")
                    [ "$ext" != img ] || expected+=("${published}.gz")
                done
            done
            ensure_img_gz
            # Everything the notes hash and sign must be named for the S3 object
            # it links. Anything else the dir has collected — a pull from before
            # assets were renamed to their published basename, or another build
            # of the same version — would be hashed under a name that is on no
            # URL we publish.
            stale=$(comm -23 <(release_files | sort -u) <(printf '%s\n' "${expected[@]}" | sort -u))
            if [ -n "$stale" ]; then
                >&2 echo "  ! not published ${VERSION} assets, but release_files would still hash and sign them — re-run with CLEAN=1 to drop them:"
                printf '      %s\n' $stale >&2
            fi
            ;;
        cli)
            gh release download -R "$REPO" "$TAG" -p 'start-cli_*' -D "$(pwd)" --clobber
            pull_apt_debs
            ;;
        deb)
            pull_apt_debs
            ;;
        npm)
            npm pack "${SDK_NPM_PACKAGE}@${VERSION}"
            ;;
        wrt)
            # Pull from the source (beta) registry — at release time the
            # version is not yet promoted into production. `registry os asset
            # get` verifies the ed25519 registry signature + blake3 commitment
            # as it streams. start-cli names its output
            # startos-<ver>_<platform>.<ext>, so move each into place under its
            # published basename (the basename of the indexed URL) to match
            # release_files().
            local slot url published tmp
            load_registry_index "$STARTWRT_SOURCE_REGISTRY"
            for slot in img squashfs; do
                url=$(asset_url "$STARTWRT_SOURCE_REGISTRY" "$slot" "$STARTWRT_PLATFORM")
                if [ -z "$url" ]; then
                    >&2 echo "  (no '$slot' asset indexed for v$VERSION, skipping)"
                    continue
                fi
                published=$(basename "$url")
                echo "  ${slot} -> ${published}"
                tmp=$(mktemp -d "$(pwd)/.get-$slot.XXXXXX")
                start-cli --registry="$STARTWRT_SOURCE_REGISTRY" registry os asset get "$slot" "$VERSION" "$STARTWRT_PLATFORM" -d "$tmp"
                mv -f "$tmp"/* "$published"
                rmdir "$tmp"
            done
            ;;
    esac
}

cmd_tag() {
    local commit="${COMMIT:-HEAD}" tag_sha pulled_sha
    tag_sha=$(tag_commit_sha)
    # Refuse to tag a commit other than the one the pulled GHA assets were built from.
    if [ -f "$(gha_commit_file)" ]; then
        pulled_sha=$(cat "$(gha_commit_file)")
        if [ "$pulled_sha" != "$tag_sha" ]; then
            >&2 echo "Release dir assets were built from commit ${pulled_sha},"
            >&2 echo "but tag ${TAG} would be cut at ${commit} (${tag_sha})."
            >&2 echo "Re-run pull-gha with the matching run, or set COMMIT=${pulled_sha}."
            exit 1
        fi
    fi
    if [ -n "$(cd "$REPO_ROOT" && git status --porcelain)" ]; then
        >&2 echo "Warning: working tree is dirty; tagging ${commit} anyway."
    fi
    echo "Tagging ${TAG} at ${commit} (${tag_sha})..."
    (cd "$REPO_ROOT" && git tag ${FORCE:+-f} "$TAG" "$commit" && git push origin ${FORCE:+-f} "refs/tags/${TAG}")
}

cmd_create_gh_release() {
    require_kind os cli deb npm wrt
    # os/cli/deb/wrt reference their pulled artifacts in the notes; npm (the SDK)
    # ships to npm and its notes are just the changelog, so it needs no release dir.
    if [ "$KIND" != npm ]; then
        enter_release_dir
        ensure_img_gz
    fi
    local notes body preamble
    notes=$(release_notes)
    echo "Creating GitHub release ${TAG}..."
    if gh release view -R "$REPO" "$TAG" >/dev/null 2>&1; then
        # release_notes() starts at $NOTES_MARKER and can regenerate nothing
        # above it, but a body may carry hand-written material there that exists
        # nowhere else — 0.4.0's "Before You Update" warning and highlights, for
        # one. `gh release edit --notes` replaces the whole body, so lift that
        # block off the live release and put it back on top. GitHub stores
        # bodies with CRLF; strip it or the splice reintroduces \r.
        body=$(gh release view -R "$REPO" "$TAG" --json body -q .body 2>/dev/null | tr -d '\r')
        preamble=$(printf '%s\n' "$body" | awk -v marker="$NOTES_MARKER" 'index($0, marker) == 1 { exit } { print }')
        if [ -n "$preamble" ]; then
            # No marker at all means the whole body is hand-written; keeping it
            # can duplicate what the generated sections say, but dropping it
            # loses the only copy, so keep and say so.
            if ! printf '%s\n' "$body" | grep -qF "$NOTES_MARKER"; then
                >&2 echo "  ! existing ${TAG} notes have no \"${NOTES_MARKER}\" heading — keeping the whole body above the generated sections; review the result"
            fi
            echo "  preserving $(printf '%s\n' "$preamble" | wc -l | tr -d ' ') hand-written line(s) above \"${NOTES_MARKER}\""
            notes="${preamble}"$'\n\n'"${notes}"
        fi
        gh release edit -R "$REPO" "$TAG" --notes "$notes"
    else
        gh release create -R "$REPO" "$TAG" --title "${PROJECT} v${VERSION}" --notes "$notes"
    fi
}

cmd_push() {
    case "$KIND" in
        os)
            enter_release_dir
            echo "Uploading OS images to ${S3_BUCKET}/v${VERSION}/ ..."
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    for file in *_"$platform"."$ext"; do
                        [ -f "$file" ] || continue
                        echo "  $file"
                        s3cmd put -P "$file" "${S3_BUCKET}/v${VERSION}/$file"
                    done
                done
            done
            ;;
        cli)
            enter_release_dir
            local files file
            mapfile -t files < <(cli_binaries)
            echo "Uploading start-cli binaries to GitHub release ${TAG}..."
            for file in "${files[@]}"; do
                gh release upload -R "$REPO" "$TAG" "$file" --clobber
            done
            publish_debs
            ;;
        deb)
            enter_release_dir
            publish_debs
            ;;
        npm)
            echo "Building and publishing ${SDK_NPM_PACKAGE}@${VERSION} to npm..."
            make -C "$REPO_ROOT/projects/start-sdk" publish ${OTP:+OTP=$OTP}
            ;;
        wrt)
            enter_release_dir
            echo "Uploading StartWRT images to ${STARTWRT_S3_BUCKET}/v${VERSION}/ ..."
            local file
            for file in $(release_files); do
                echo "  $file"
                s3cmd put -P "$file" "${STARTWRT_S3_BUCKET}/v${VERSION}/$file"
            done
            ;;
    esac
}

# Put the compressed copy of each raw .img beside the bare image on S3.
#
# This adds an object rather than replacing one: the registry keeps pointing at
# the bare .img (see ensure_img_gz), and only the release-notes download link
# moves to the .gz. That link is the indexed URL + ".gz", so the key has to be
# the indexed basename + ".gz" — the guards below exist because any other name
# yields notes that link to nothing.
#
# The local img is picked by content, not name: one platform can be hotfixed
# and re-indexed at a different commit than the rest of the release, and a
# start-cli download drops the hash entirely, so the git hash a basename embeds
# proves nothing. The registry's signed blake3 commitment is what the .gz must
# decompress to; match against that, then rename to the indexed basename so
# checksums and signatures agree with the object the notes link.
cmd_push_gz() {
    require_kind os
    enter_release_dir
    ensure_img_gz
    echo "Uploading compressed OS images to ${S3_BUCKET}/v${VERSION}/ ..."
    local platform ext url published expected img f
    for platform in $OS_PLATFORMS; do
        for ext in $(os_image_exts "$platform"); do
            [ "$ext" = img ] || continue
            url=$(asset_url "$STARTOS_SOURCE_REGISTRY" img "$platform")
            if [ -z "$url" ]; then
                >&2 echo "  ✗ no ${platform} img indexed in ${STARTOS_SOURCE_REGISTRY}"
                exit 1
            fi
            published=$(basename "$url")
            if [ "$url" != "${S3_CDN}/v${VERSION}/${published}" ]; then
                >&2 echo "  ✗ ${platform} img is indexed at ${url}, outside ${S3_CDN}/v${VERSION}/ — a .gz uploaded to ${S3_BUCKET} would not be reachable at that URL + .gz"
                exit 1
            fi
            expected=$(asset_commitment_b3 "$STARTOS_SOURCE_REGISTRY" img "$platform")
            if [ -z "$expected" ]; then
                >&2 echo "  ✗ ${published} has no blake3 commitment in ${STARTOS_SOURCE_REGISTRY}"
                exit 1
            fi
            img=
            for f in "$published" *.img; do
                [ -f "$f" ] || continue
                if [ "$(b3sum --no-names "$f")" = "$expected" ]; then
                    img=$f
                    break
                fi
            done
            if [ -z "$img" ]; then
                >&2 echo "  ✗ no img in $(release_dir) matches the blake3 ${STARTOS_SOURCE_REGISTRY} commits to for ${published} — run 'pull' (or 'pull-gha' with the run that built the indexed img) first"
                exit 1
            fi
            if [ "$img" != "$published" ]; then
                echo "  ${img} -> ${published} (renaming to the indexed basename)"
                mv -f "$img" "$published"
                [ ! -f "${img}.gz" ] || mv -f "${img}.gz" "${published}.gz"
                ensure_img_gz
            fi
            echo "  ${published}.gz"
            s3cmd put -P "${published}.gz" "${S3_BUCKET}/v${VERSION}/${published}.gz"
        done
    done
}

cmd_index() {
    require_kind os wrt
    case "$KIND" in
        os)
            # Promote the version (+ every iso/squashfs/img asset) from the source
            # registry into production. This copies the index entries and re-signs
            # the commitments with the developer key — the images stay on the
            # shared S3 bucket, so nothing is re-uploaded.
            echo "Promoting OS ${VERSION}: ${STARTOS_SOURCE_REGISTRY} -> ${STARTOS_TARGET_REGISTRY} ..."
            start-cli registry os promote --from "$STARTOS_SOURCE_REGISTRY" --to "$STARTOS_TARGET_REGISTRY" "$VERSION"
            ;;
        wrt)
            # Promote the version (+ its img/squashfs assets) from the source
            # (beta) registry into production — the same index-copy +
            # developer-key re-sign as the OS; the images stay on the StartWRT
            # S3 bucket, so nothing is re-uploaded.
            echo "Promoting StartWRT ${VERSION}: ${STARTWRT_SOURCE_REGISTRY} -> ${STARTWRT_TARGET_REGISTRY} ..."
            start-cli registry os promote --from "$STARTWRT_SOURCE_REGISTRY" --to "$STARTWRT_TARGET_REGISTRY" "$VERSION"
            ;;
    esac
}

cmd_register() {
    require_kind wrt
    # Register + index a CI build into the source (beta) registry, where beta
    # routers soak it before `release` promotes it into production. Normally
    # the CI deploy job (start-wrt.yaml) does this itself right after the S3
    # upload — this is the manual fallback (same commands; keep in sync). Run
    # pull-gha first: registering needs the image files locally to compute the
    # signed blake3 commitments. The compat range is the set of installed
    # versions allowed to upgrade to this one.
    enter_release_dir
    echo "Registering StartWRT ${VERSION} in ${STARTWRT_SOURCE_REGISTRY}..."
    start-cli --registry="$STARTWRT_SOURCE_REGISTRY" registry os version add \
        "$VERSION" "v$VERSION" '' "${STARTWRT_COMPAT_FLOOR} <=$VERSION"

    # start-cli infers the asset slot from the file extension and only accepts
    # iso/img/squashfs. Both images ship gzipped, so present each under a
    # hardlink whose extension names its slot: the sdcard image as .img (the
    # fresh-install slot) and the sysupgrade image as .squashfs (start-os's
    # update-asset slot). The indexed URLs still point at the honestly-named
    # .img.gz files on S3 (the registry only requires the URL's bytes to match
    # the signed blake3 commitment, which the hardlinks share).
    local file index_file
    for file in $(release_files); do
        case "$file" in
            *-sdcard.img.gz) index_file="${file%.img.gz}.img" ;;
            *-sysupgrade.img.gz) index_file="${file%.img.gz}.squashfs" ;;
            *) index_file="$file" ;;
        esac
        [ "$index_file" = "$file" ] || ln -f "$file" "$index_file"
        echo "Indexing $file for platform ${STARTWRT_PLATFORM}..."
        start-cli --registry="$STARTWRT_SOURCE_REGISTRY" registry os asset add \
            --platform="$STARTWRT_PLATFORM" --version="$VERSION" \
            "$index_file" "$STARTWRT_S3_CDN/v$VERSION/$file"
    done
}

cmd_sign() {
    require_kind os cli deb wrt
    enter_release_dir
    ensure_img_gz
    resolve_gh_user

    local files file
    mapfile -t files < <(release_files)
    mkdir -p signatures
    for file in "${files[@]}"; do
        gpg -u $START9_GPG_KEY --detach-sign --armor -o "signatures/${file}.start9.asc" "$file"
        if [ -n "$GH_USER" ] && [ -n "$GH_GPG_KEY" ]; then
            gpg -u "$GH_GPG_KEY" --detach-sign --armor -o "signatures/${file}.${GH_USER}.asc" "$file"
        fi
    done

    gpg --export -a $START9_GPG_KEY > signatures/start9.key.asc
    if [ -n "$GH_USER" ] && [ -n "$GH_GPG_KEY" ]; then
        gpg --export -a "$GH_GPG_KEY" > "signatures/${GH_USER}.key.asc"
    else
        >&2 echo 'Warning: could not determine GitHub user or GPG signing key, skipping personal signature'
    fi
    tar -czf signatures.tar.gz -C signatures .

    gh release upload -R "$REPO" "$TAG" signatures.tar.gz --clobber
}

cmd_cosign() {
    require_kind os cli deb wrt
    enter_release_dir
    ensure_img_gz
    resolve_gh_user

    if [ -z "$GH_USER" ] || [ -z "$GH_GPG_KEY" ]; then
        >&2 echo 'Error: could not determine GitHub user or GPG signing key'
        >&2 echo "Set GH_USER and/or configure git user.signingkey"
        exit 1
    fi

    echo "Downloading existing signatures..."
    gh release download -R "$REPO" "$TAG" -p "signatures.tar.gz" -D "$(pwd)" --clobber
    mkdir -p signatures
    tar -xzf signatures.tar.gz -C signatures

    echo "Adding personal signatures as $GH_USER..."
    local files file
    mapfile -t files < <(release_files)
    for file in "${files[@]}"; do
        gpg -u "$GH_GPG_KEY" --detach-sign --armor -o "signatures/${file}.${GH_USER}.asc" "$file"
    done
    gpg --export -a "$GH_GPG_KEY" > "signatures/${GH_USER}.key.asc"

    tar -czf signatures.tar.gz -C signatures .
    gh release upload -R "$REPO" "$TAG" signatures.tar.gz --clobber
    echo "Done. Personal signatures for $GH_USER added to ${TAG}."
}

# Compose the release-notes body for the current project.
release_notes() {
    echo "$NOTES_MARKER"
    echo
    changelog_section
    echo

    local platform
    case "$KIND" in
        os)
            echo "## Image Downloads"
            echo
            local ext url
            load_registry_index "$STARTOS_SOURCE_REGISTRY"
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    # squashfs is the over-the-air update asset, not a download.
                    [ "$ext" != squashfs ] || continue
                    url=$(asset_url "$STARTOS_SOURCE_REGISTRY" "$ext" "$platform")
                    [ -n "$url" ] || continue
                    if [ "$ext" = img ]; then
                        echo "- [$(os_platform_label "$platform")](${url}.gz \"gzip-compressed — Raspberry Pi Imager and balenaEtcher flash it without unpacking\")"
                    else
                        echo "- [$(os_platform_label "$platform")]($url)"
                    fi
                done
            done
            echo
            local imgs
            mapfile -t imgs < <(release_files)
            checksum_block "OS Images" "${imgs[@]}"
            ;;
        cli)
            local bins debs
            mapfile -t bins < <(cli_binaries)
            checksum_block "start-cli" "${bins[@]}"
            mapfile -t debs < <(deb_files)
            checksum_block "start-cli packages" "${debs[@]}"
            ;;
        deb)
            local debs
            mapfile -t debs < <(deb_files)
            checksum_block "${PROJECT} packages" "${debs[@]}"
            ;;
        wrt)
            echo "## Image Downloads"
            echo
            local sdcard sysupgrade imgs
            load_registry_index "$STARTWRT_SOURCE_REGISTRY"
            sdcard=$(asset_url "$STARTWRT_SOURCE_REGISTRY" img "$STARTWRT_PLATFORM")
            sysupgrade=$(asset_url "$STARTWRT_SOURCE_REGISTRY" squashfs "$STARTWRT_PLATFORM")
            if [ -n "$sdcard" ]; then
                echo "- [SD card image (fresh install)]($sdcard \"Write to microSD/eMMC to flash a new device\")"
            fi
            if [ -n "$sysupgrade" ]; then
                echo "- [Sysupgrade image (OTA update)]($sysupgrade \"In-place upgrade via OpenWrt sysupgrade\")"
            fi
            echo
            mapfile -t imgs < <(release_files)
            checksum_block "StartWRT" "${imgs[@]}"
            ;;
    esac
}

checksum_block() {
    local title=$1
    shift
    [ "$#" -gt 0 ] || return 0
    echo "## ${title} Checksums"
    echo
    echo "### SHA-256"
    echo '```'
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$@"
    else
        # macOS ships shasum but not coreutils' sha256sum; same output format.
        shasum -a 256 "$@"
    fi
    echo '```'
    echo
    echo "### BLAKE-3"
    echo '```'
    b3sum "$@"
    echo '```'
}

cmd_notes() {
    require_kind os cli deb npm wrt
    if [ "$KIND" != npm ]; then
        enter_release_dir
        ensure_img_gz
    fi
    release_notes
}

cmd_release() {
    case "$KIND" in
        os)
            # CI already uploaded the images to the shared S3 bucket and indexed
            # them into alpha (and alpha->beta was promoted manually), so there's
            # no push here. Pull the promoted images to build the release notes +
            # sign them, and `index` promotes them from source into production.
            # push-gz is the one upload left: the compressed .img the notes link,
            # which has to be on S3 before the notes reference it.
            cmd_pre_check
            cmd_pull
            cmd_push_gz
            cmd_tag
            cmd_create_gh_release
            cmd_index
            cmd_sign
            ;;
        cli | deb)
            cmd_pre_check
            cmd_pull_gha
            cmd_tag
            cmd_create_gh_release
            cmd_push
            cmd_sign
            ;;
        npm)
            # create-gh-release before push: everything idempotent runs ahead of
            # the one irreversible step (npm publish can't be re-run for a version).
            cmd_pre_check
            cmd_tag
            cmd_create_gh_release
            cmd_push
            ;;
        wrt)
            # CI (start-wrt.yaml `deploy`) uploaded the images to S3 and
            # registered + indexed them into the source (beta) registry, where
            # beta routers soaked the version — so there's no push here. Pull
            # the registered images (signature-verified) to build the release
            # notes + sign them, tag, cut the GitHub release, and `index`
            # promotes them from source into production.
            cmd_pre_check
            cmd_pull
            cmd_tag
            cmd_create_gh_release
            cmd_index
            cmd_sign
            ;;
    esac
}

usage() {
    cat << 'EOF'
Usage: manage-release.sh <subcommand> <project>

Projects:
  start-os        OS images (iso/squashfs, raspberrypi img) -> S3 + registry
                  OS index; the raw img also gets a .gz on S3 for the notes
  start-cli       per-triple binaries -> GitHub release; per-arch .deb -> apt + GitHub
  start-tunnel    per-arch .deb -> apt repo + GitHub release
  start-registry  per-arch .deb -> apt repo + GitHub release
  start-sdk       npm package -> npm + GitHub release
  start-wrt       flashable images (sdcard + sysupgrade .img.gz) -> S3 +
                  StartWRT registries (CI's deploy indexes into beta; `release`
                  promotes beta -> production)

Version is read from the project's manifest (Cargo.toml — for start-wrt the ctrl
crate's — or package.json for start-sdk); the git tag / GitHub release is
<project>/v<version>.

Subcommands:
  pre-check          Verify the changelog documents this version and that the
                     version is not already tagged/released.
  pull-gha           Download build artifacts from a GitHub Actions run.
                     Fails unless the run built the commit being tagged
                     (COMMIT, default HEAD).
                     (os/cli/deb/wrt; set RUN_ID or you'll be prompted.)
  pull               Download the released assets from their official location
                     (registry / apt repo / GitHub release / npm).
  tag                Create and push the <project>/v<version> git tag. Fails if
                     pull-gha'd assets were built from a different commit.
  create-gh-release  Create (or update) the GitHub release with notes.
                     (all projects.)
  push               Upload artifacts to their destination (S3 for os, GitHub
                     release + apt for cli/deb, npm publish for sdk). For os this
                     normally runs in CI; use it for a manual re-publish.
  push-gz            os only: compress each raw .img and upload the .gz beside
                     the bare image on S3 — the registry keeps indexing the bare
                     image; only the release-notes download link uses the .gz.
                     Run 'pull' (or 'pull-gha') first.
  index              Promote the version from the source (beta) registry into
                     the production registry. (os: CI indexes alpha; alpha->beta
                     is promoted manually. wrt: CI indexes into beta.)
  register           wrt only: register + index a CI build's images into the
                     source (beta) registry, pointing at their S3/CDN URLs.
                     Manual fallback — the CI deploy normally does this. Run
                     pull-gha first; beta routers then soak the version
                     until `release` promotes it.
  sign               Sign artifacts with the Start9 org key (+ personal key if
                     available) and upload signatures.tar.gz. (os/cli/deb/wrt.)
  cosign             Add your personal GPG signature to an existing release's
                     signatures.tar.gz. (os/cli/deb/wrt; run 'pull' first.)
  notes              Print the release notes to stdout. (all projects.)
  release            Run the full applicable pipeline for the project.

Environment variables:
  VERSION                  Override the version (default: read from the manifest)
  RUN_ID                   GitHub Actions run id/url for pull-gha
  COMMIT                   Commit to tag (default: HEAD)
  FORCE                    Set to 1 to re-release an already-released version:
                           force-move the tag and downgrade pre-check's "already
                           released" failures to warnings (idempotent steps only;
                           npm republish always fails)
  CLEAN                    Set to 1 to wipe and recreate the release directory
  GH_USER                  Override GitHub username (default: autodetected via gh)
  OTP                      npm one-time password for start-sdk publish
                           (prompted for at publish time if unset)

Registries are scoped per project (the OS and StartWRT promote source -> target):
  STARTOS_SOURCE_REGISTRY   registry the OS release pulls/promotes from (default: beta)
  STARTOS_TARGET_REGISTRY   registry the OS release promotes into (default: production)
  STARTWRT_SOURCE_REGISTRY  registry CI (or `register`) indexes into and the
                            StartWRT release pulls/promotes from (default: beta)
  STARTWRT_TARGET_REGISTRY  registry the StartWRT release promotes into
                            (default: production)
  STARTWRT_COMPAT_FLOOR     oldest version allowed to upgrade to this StartWRT
                            release (default: >=0.1.0-beta.1)
EOF
}

# --- Dispatch ---

SUBCOMMAND="${1:-}"
PROJECT="${2:-}"

if [ -z "$SUBCOMMAND" ] || [ "$SUBCOMMAND" = "-h" ] || [ "$SUBCOMMAND" = "--help" ]; then
    usage
    exit 0
fi

if ! KIND=$(project_kind "$PROJECT"); then
    >&2 echo "Unknown or missing project: '${PROJECT}'"
    >&2 echo "Projects: ${PROJECTS}"
    exit 2
fi

VERSION="${VERSION:-$(derive_version "$PROJECT")}"
if [ -z "$VERSION" ]; then
    >&2 echo "Could not derive version for ${PROJECT}"
    exit 1
fi
TAG="${PROJECT}/v${VERSION}"

case "$SUBCOMMAND" in
    pre-check) cmd_pre_check ;;
    pull-gha) cmd_pull_gha ;;
    pull) cmd_pull ;;
    tag) cmd_tag ;;
    create-gh-release) cmd_create_gh_release ;;
    push) cmd_push ;;
    push-gz) cmd_push_gz ;;
    index) cmd_index ;;
    register) cmd_register ;;
    sign) cmd_sign ;;
    cosign) cmd_cosign ;;
    notes) cmd_notes ;;
    release) cmd_release ;;
    *)
        >&2 echo "Unknown subcommand: '${SUBCOMMAND}'"
        usage
        exit 2
        ;;
esac
