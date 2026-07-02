#!/bin/bash
#
# manage-release.sh — drive a monorepo product through its release steps.
#
# Usage: ./scripts/manage-release.sh <subcommand> <project>
#
# See usage() for the subcommands. The <project> is one of the monorepo's
# releasable products; its version is read from that product's canonical
# manifest (Cargo.toml for the Rust products, package.json for the SDK) and its
# git tag / GitHub release is <project>_v<version>.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

REPO="Start9Labs/start-technologies"
# Registries are scoped per project (<PROJECT>_SOURCE/TARGET_REGISTRY); only the
# OS promotes between registries. The OS chain: CI indexes images into alpha;
# alpha -> beta is promoted manually, out of band; the full `release` promotes
# the source (beta) -> target (production). Override either per run.
STARTOS_SOURCE_REGISTRY="${STARTOS_SOURCE_REGISTRY:-https://beta-registry.start9.com}"
STARTOS_TARGET_REGISTRY="${STARTOS_TARGET_REGISTRY:-https://registry.start9.com}"
S3_BUCKET="s3://startos-images"
S3_CDN="https://startos-images.nyc3.cdn.digitaloceanspaces.com"
START9_GPG_KEY="2D63C217"
SDK_NPM_PACKAGE="@start9labs/start-sdk"

APT_BASE_URL="https://start9-debs.nyc3.digitaloceanspaces.com"
APT_SUITE="stable"
APT_COMPONENT="main"

# Every OS image platform. Most ship an iso + squashfs; raspberrypi ships a
# flashable img + squashfs (no iso). See os_image_exts.
OS_PLATFORMS="x86_64 x86_64-nonfree x86_64-nvidia aarch64 aarch64-nonfree aarch64-nvidia raspberrypi riscv64 riscv64-nonfree"
CLI_TRIPLES="x86_64-unknown-linux-musl x86_64-apple-darwin aarch64-unknown-linux-musl aarch64-apple-darwin riscv64gc-unknown-linux-musl"
DEB_ARCHES="x86_64 aarch64 riscv64"

PROJECTS="start-os start-cli start-tunnel start-registry start-sdk"

# --- Project metadata ---

project_kind() {
    case "$1" in
        start-os) echo os ;;
        start-cli) echo cli ;;
        start-tunnel | start-registry) echo deb ;;
        start-sdk) echo npm ;;
        *) return 1 ;;
    esac
}

derive_version() {
    local project=$1 version
    if [ "$(project_kind "$project")" = npm ]; then
        jq -r .version "$REPO_ROOT/projects/$project/package.json"
        return
    fi
    local toml="$REPO_ROOT/projects/$project/Cargo.toml"
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

parse_run_id() {
    local val="$1"
    if [[ "$val" =~ /actions/runs/([0-9]+) ]]; then
        echo "${BASH_REMATCH[1]}"
    else
        echo "$val"
    fi
}

release_dir() { echo "$HOME/Downloads/${PROJECT}_v${VERSION}"; }

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

# List every file the project ships (for signing / checksums), one per line.
release_files() {
    local f
    case "$KIND" in
        os) for f in *.iso *.img *.squashfs; do [ -f "$f" ] && echo "$f"; done ;;
        cli) cli_binaries; deb_files ;;
        deb) deb_files ;;
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

    # 1. Changelog must document this version explicitly (not just Unreleased).
    local changelog ver_re
    changelog=$(changelog_path "$PROJECT")
    ver_re=${VERSION//./\\.}
    if [ ! -f "$changelog" ]; then
        >&2 echo "  ✗ no CHANGELOG.md at $changelog"
        errors=1
    elif ! grep -qE "^##[[:space:]]+\[?${ver_re}(]| |\$)" "$changelog"; then
        >&2 echo "  ✗ CHANGELOG.md has no explicit heading for ${VERSION}"
        errors=1
    else
        echo "  ✓ changelog documents ${VERSION}"
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
        os | cli | deb)
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
            local idx missing platform ext
            idx=$(start-cli --registry="$STARTOS_SOURCE_REGISTRY" registry os index 2>/dev/null || echo '{}')
            if ! echo "$idx" | jq -e ".versions[\"$VERSION\"]" >/dev/null 2>&1; then
                >&2 echo "  ✗ OS ${VERSION} not in source registry ${STARTOS_SOURCE_REGISTRY} — promote it there first"
                errors=1
            else
                missing=""
                for platform in $OS_PLATFORMS; do
                    for ext in $(os_image_exts "$platform"); do
                        echo "$idx" | jq -e ".versions[\"$VERSION\"].${ext}[\"$platform\"].urls[0]" >/dev/null 2>&1 \
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
            # promoting re-signs registry commitments with the developer key.
            if [ -f "$HOME/.startos/developer.key.pem" ]; then
                echo "  ✓ developer key present"
            else
                >&2 echo "  ✗ ~/.startos/developer.key.pem missing (needed to promote to the registry)"
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
    esac

    # gh + the Start9 signing key are needed by every os/cli/deb release
    # (create-gh-release, upload, sign — and the apt Release signature for debs).
    if [ "$KIND" != npm ]; then
        if gh auth status >/dev/null 2>&1; then
            echo "  ✓ gh authenticated"
        else
            >&2 echo "  ✗ gh not authenticated (run: gh auth login)"
            errors=1
        fi
        if gpg --list-secret-keys "$START9_GPG_KEY" >/dev/null 2>&1; then
            echo "  ✓ Start9 signing key ${START9_GPG_KEY} present"
        else
            >&2 echo "  ✗ Start9 GPG secret key ${START9_GPG_KEY} not in keyring (needed to sign)"
            errors=1
        fi
    fi

    # cli/deb also publish debs to the apt repo, which needs s3cmd + credentials.
    if [ "$KIND" = cli ] || [ "$KIND" = deb ]; then
        if command -v s3cmd >/dev/null 2>&1; then
            echo "  ✓ s3cmd available"
        else
            >&2 echo "  ✗ s3cmd not installed (needed to publish debs to apt)"
            errors=1
        fi
        if [ -f "$HOME/.s3cfg" ] || { [ -n "${S3_ACCESS_KEY:-}" ] && [ -n "${S3_SECRET_KEY:-}" ]; }; then
            echo "  ✓ s3 credentials configured"
        else
            >&2 echo "  ! no ~/.s3cfg and S3_ACCESS_KEY/S3_SECRET_KEY unset — apt publish may fail"
        fi
    fi

    if [ "$errors" -ne 0 ]; then
        >&2 echo "Pre-check failed."
        exit 1
    fi
    echo "Pre-check passed."
}

cmd_pull_gha() {
    require_kind os cli deb

    if [ -z "${RUN_ID:-}" ]; then
        read -rp "RUN_ID (GitHub Actions run for ${PROJECT}): " RUN_ID
    fi
    RUN_ID=$(parse_run_id "${RUN_ID:-}")
    if [ -z "$RUN_ID" ]; then
        >&2 echo "RUN_ID is required"
        exit 2
    fi

    ensure_release_dir
    echo "Downloading ${PROJECT} artifacts from run ${RUN_ID}..."

    case "$KIND" in
        os)
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    echo "  ${platform}.${ext}"
                    gh run download -R "$REPO" "$RUN_ID" -n "${platform}.${ext}" -D "$(pwd)"
                done
            done
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
    esac
}

cmd_pull() {
    ensure_release_dir
    echo "Downloading released ${PROJECT} v${VERSION} from its official location..."

    case "$KIND" in
        os)
            for platform in $OS_PLATFORMS; do
                for ext in $(os_image_exts "$platform"); do
                    echo "  ${ext} ${platform}"
                    start-cli --registry="$STARTOS_SOURCE_REGISTRY" registry os asset get "$ext" "$VERSION" "$platform" -d "$(pwd)"
                done
            done
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
    esac
}

cmd_tag() {
    local commit="${COMMIT:-HEAD}"
    if [ -n "$(cd "$REPO_ROOT" && git status --porcelain)" ]; then
        >&2 echo "Warning: working tree is dirty; tagging ${commit} anyway."
    fi
    echo "Tagging ${TAG} at ${commit}..."
    (cd "$REPO_ROOT" && git tag ${FORCE:+-f} "$TAG" "$commit" && git push origin ${FORCE:+-f} "refs/tags/${TAG}")
}

cmd_create_gh_release() {
    require_kind os cli deb
    enter_release_dir
    local notes
    notes=$(release_notes)
    echo "Creating GitHub release ${TAG}..."
    if gh release view -R "$REPO" "$TAG" >/dev/null 2>&1; then
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
    esac
}

cmd_index() {
    require_kind os
    # Promote the version (+ every iso/squashfs/img asset) from the source
    # registry into production. This copies the index entries and re-signs the
    # commitments with the developer key — the images stay on the shared S3
    # bucket, so nothing is re-uploaded.
    echo "Promoting OS ${VERSION}: ${STARTOS_SOURCE_REGISTRY} -> ${STARTOS_TARGET_REGISTRY} ..."
    start-cli registry os promote --from "$STARTOS_SOURCE_REGISTRY" --to "$STARTOS_TARGET_REGISTRY" "$VERSION"
}

cmd_sign() {
    require_kind os cli deb
    enter_release_dir
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
    require_kind os cli deb
    enter_release_dir
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
    echo "## What's Changed"
    echo
    changelog_section
    echo

    local platform file
    case "$KIND" in
        os)
            echo "## Image Downloads"
            echo
            for platform in $OS_PLATFORMS; do
                for file in *_"$platform".iso *_"$platform".img; do
                    [ -f "$file" ] || continue
                    echo "- [$(os_platform_label "$platform")]($S3_CDN/v$VERSION/$file)"
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
    sha256sum "$@" 2>/dev/null || true
    echo '```'
    echo
    echo "### BLAKE-3"
    echo '```'
    b3sum "$@" 2>/dev/null || true
    echo '```'
}

cmd_notes() {
    require_kind os cli deb
    enter_release_dir
    release_notes
}

cmd_release() {
    case "$KIND" in
        os)
            # CI already uploaded the images to the shared S3 bucket and indexed
            # them into alpha (and alpha->beta was promoted manually), so there's
            # no push here. Pull the promoted images to build the release notes +
            # sign them, and `index` promotes them from source into production.
            cmd_pre_check
            cmd_pull
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
            cmd_pre_check
            cmd_tag
            cmd_push
            ;;
    esac
}

usage() {
    cat << 'EOF'
Usage: manage-release.sh <subcommand> <project>

Projects:
  start-os        OS images (iso/squashfs) -> S3 + registry OS index
  start-cli       per-triple binaries -> GitHub release; per-arch .deb -> apt + GitHub
  start-tunnel    per-arch .deb -> apt repo + GitHub release
  start-registry  per-arch .deb -> apt repo + GitHub release
  start-sdk       npm package -> npm

Version is read from the project's manifest (Cargo.toml, or package.json for
start-sdk); the git tag / GitHub release is <project>_v<version>.

Subcommands:
  pre-check          Verify the changelog documents this version and that the
                     version is not already tagged/released.
  pull-gha           Download build artifacts from a GitHub Actions run.
                     (os/cli/deb; set RUN_ID or you'll be prompted.)
  pull               Download the released assets from their official location
                     (registry / apt repo / GitHub release / npm).
  tag                Create and push the <project>_v<version> git tag.
  create-gh-release  Create (or update) the GitHub release with notes.
                     (os/cli/deb.)
  push               Upload artifacts to their destination (S3 for os, GitHub
                     release + apt for cli/deb, npm publish for sdk). For os this
                     normally runs in CI; use it for a manual re-publish.
  index              Promote the OS version from the source registry into the
                     production registry (os only). CI indexes alpha; alpha->beta
                     is promoted manually; this does source (beta) -> prod.
  sign               Sign artifacts with the Start9 org key (+ personal key if
                     available) and upload signatures.tar.gz. (os/cli/deb.)
  cosign             Add your personal GPG signature to an existing release's
                     signatures.tar.gz. (os/cli/deb; run 'pull' first.)
  notes              Print the release notes to stdout. (os/cli/deb.)
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
  OTP                      npm one-time password (start-sdk publish)

Registries are scoped per project (only the OS promotes between registries):
  STARTOS_SOURCE_REGISTRY  registry the OS release pulls/promotes from (default: beta)
  STARTOS_TARGET_REGISTRY  registry the OS release promotes into (default: production)
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
TAG="${PROJECT}_v${VERSION}"

case "$SUBCOMMAND" in
    pre-check) cmd_pre_check ;;
    pull-gha) cmd_pull_gha ;;
    pull) cmd_pull ;;
    tag) cmd_tag ;;
    create-gh-release) cmd_create_gh_release ;;
    push) cmd_push ;;
    index) cmd_index ;;
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
