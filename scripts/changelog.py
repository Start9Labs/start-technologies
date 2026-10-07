#!/usr/bin/env python3
"""Validate, render and consume product changelog fragments."""

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import subprocess
import sys


from changelog_version import MANIFESTS, TIERS, latest_release, manifest_version, next_version, parse_version


KINDS = ("added", "changed", "deprecated", "removed", "fixed", "security")
FILENAME = re.compile(
    r"(" + "|".join(TIERS) + r")-(" + "|".join(KINDS) + r")-[a-z0-9]+(?:-[a-z0-9]+)*\.md"
)
PRODUCT = re.compile(r"projects/([a-z0-9]+(?:-[a-z0-9]+)*)")


@dataclass(frozen=True)
class Fragment:
    path: str
    tier: str
    kind: str
    data: bytes
    body: str


@dataclass
class Release:
    product: str
    version: str
    commit: str
    fragments: dict


def git(*args):
    return subprocess.check_output(["git", *args], stderr=subprocess.PIPE)


def product_path(value):
    value = value.rstrip("/")
    if not PRODUCT.fullmatch(value) or Path(value).name not in MANIFESTS:
        raise ValueError(f"expected projects/<product>: {value!r}")
    return value


def parse_fragment(path, data):
    match = FILENAME.fullmatch(Path(path).name)
    if not match:
        raise ValueError(f"{path}: expected <{'|'.join(TIERS)}>-<kind>-<name>.md")
    body = data.decode("utf-8").strip()
    if not body:
        raise ValueError(f"{path}: empty fragment")
    if re.search(r"^\s{0,3}#{1,6}(?:\s|$)", body, re.MULTILINE) or re.search(
        r"^\s{0,3}(?:=+|-+)\s*$", body, re.MULTILINE
    ):
        raise ValueError(f"{path}: headings are not allowed")
    if not re.match(r"[-+*]\s+\S", body):
        raise ValueError(f"{path}: body must start with a Markdown bullet item")
    return Fragment(path, match[1], match[2], data, body)


def has_symlink(path):
    return any(part.is_symlink() for part in (path, *path.parents))


def working_fragments(product):
    directory = Path(product) / "changelog"
    if has_symlink(directory):
        raise ValueError(f"{directory}: symlinks are not allowed")
    if not directory.exists():
        return {}
    if not directory.is_dir():
        raise ValueError(f"{directory}: expected directory")
    fragments = {}
    for path in sorted(directory.iterdir()):
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"{path}: expected a regular file, not a symlink or directory")
        fragments[str(path)] = parse_fragment(str(path), path.read_bytes())
    return fragments


def tree_entry(ref, path):
    entries = git("ls-tree", "-z", ref, "--", path).split(b"\0")
    return entries[0] if entries[0] else None


def regular_blob(metadata, location):
    mode, kind, blob = metadata.split()
    if kind != b"blob" or mode not in (b"100644", b"100755"):
        raise ValueError(f"{location}: expected a regular file")
    return git("cat-file", "blob", blob.decode())


def tagged_fragments(product, ref):
    directory = f"{product}/changelog"
    entry = tree_entry(ref, directory)
    if entry is None:
        return {}
    if not entry.startswith(b"040000 tree "):
        raise ValueError(f"{ref}:{directory}: expected directory")
    fragments = {}
    for entry in git("ls-tree", "-z", f"{ref}:{directory}").split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        path = f"{directory}/{name.decode('utf-8')}"
        fragments[path] = parse_fragment(path, regular_blob(metadata, f"{ref}:{path}"))
    return fragments


def tagged_history(product, ref):
    path = f"{product}/CHANGELOG.md"
    entry = tree_entry(ref, path)
    if entry is None:
        return b""
    metadata, _ = entry.split(b"\t", 1)
    return regular_blob(metadata, f"{ref}:{path}")


def origin_tags():
    tags = {}
    for line in git("ls-remote", "--tags", "origin").decode().splitlines():
        sha, name = line.split("\t")
        name = name.removeprefix("refs/tags/")
        peeled = name.endswith("^{}")
        name = name.removesuffix("^{}")
        match = re.fullmatch(r"([a-z0-9]+(?:-[a-z0-9]+)*)/v(.+)", name)
        if not match or match[1] not in MANIFESTS:
            continue
        parse_version(match[1], match[2])
        if name not in tags or peeled:
            tags[name] = sha
    return tags


def origin_releases(ref, product=None, tags=None):
    if tags is None:
        tags = origin_tags()
    ordered = git("rev-list", "--topo-order", "--reverse", ref).decode().splitlines()
    positions = {commit: i for i, commit in enumerate(ordered)}
    releases = []
    for tag, sha in tags.items():
        if product and not tag.startswith(f"{Path(product).name}/v"):
            continue
        commit = git("rev-parse", "--verify", f"{sha}^{{commit}}").decode().strip()
        if commit not in positions:
            continue
        name, version = tag.split("/v", 1)
        path = f"projects/{name}"
        releases.append(Release(path, version, commit, tagged_fragments(path, commit)))
    return sorted(releases, key=lambda release: (positions[release.commit], release.product, release.version))


def is_ancestor(older, newer):
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", older, newer], stderr=subprocess.PIPE
    )
    if result.returncode not in (0, 1):
        raise ValueError(result.stderr.decode().strip())
    return result.returncode == 0


def deleted_between(path, source, target):
    return bool(git("log", "--format=%H", "--diff-filter=D", "--no-renames",
                    f"{source}..{target}", "--", path))


def owned_fragments(release, earlier):
    ancestors = [previous for previous in earlier
                 if previous.product == release.product and is_ancestor(previous.commit, release.commit)]

    def inherited(fragment):
        for previous in ancestors:
            old = previous.fragments.get(fragment.path)
            if old is not None and old.data == fragment.data:
                if not deleted_between(fragment.path, previous.commit, release.commit):
                    return True
        return False

    return [fragment for fragment in release.fragments.values() if not inherited(fragment)]


def release_section(version, fragments):
    lines = [f"## [{version}]", ""]
    for kind in KINDS:
        group = sorted((item for item in fragments if item.kind == kind), key=lambda item: item.path)
        if group:
            lines.extend([f"### {kind.title()}", ""])
            for fragment in group:
                lines.extend([fragment.body, ""])
    return ("\n".join(lines) + "\n").encode()


def prepend_section(history, version, fragments):
    if not fragments:
        return history
    section = release_section(version, fragments)
    heading = re.search(rb"^##\s+\[" + re.escape(version.encode()) + rb"\](?:[^\r\n]*)\r?\n", history, re.MULTILINE)
    if heading:
        next_heading = re.search(rb"^## ", history[heading.end():], re.MULTILINE)
        end = heading.end() + next_heading.start() if next_heading else len(history)
        if history[heading.start():end].strip() != section.strip():
            raise ValueError(f"release {version} already has a different compiled changelog; do not move released tags")
        return history
    first = re.search(rb"^## ", history, re.MULTILINE)
    position = first.start() if first else len(history)
    prefix = history[:position]
    separator = b"" if not prefix or re.search(rb"(?:\r?\n){2}$", prefix) else b"\n" if prefix.endswith(b"\n") else b"\n\n"
    return prefix + separator + section + history[position:]


def compile_releases(history, releases):
    earlier = []
    consumed = []
    for release in releases:
        owned = owned_fragments(release, earlier)
        history = prepend_section(history, release.version, owned)
        consumed.extend((release.commit, fragment) for fragment in owned)
        earlier.append(release)
    return history, consumed


def render(product, version, ref=None):
    parse_version(Path(product).name, version)
    commit = git("rev-parse", "--verify", f"{ref or 'HEAD'}^{{commit}}").decode().strip()
    if ref:
        history = tagged_history(product, commit)
        fragments = tagged_fragments(product, commit)
    else:
        path = Path(product) / "CHANGELOG.md"
        history = path.read_bytes() if path.exists() else b""
        fragments = working_fragments(product)
    releases = [item for item in origin_releases(commit, product) if item.version != version]
    releases.append(Release(product, version, commit, fragments))
    return compile_releases(history, releases)[0]


def check_manifest_version(product, version=None):
    name = Path(product).name
    version = manifest_version(Path.cwd(), name) if version is None else version
    parse_version(name, version)
    fragments = working_fragments(product)
    tags = origin_tags()
    baseline = latest_release(name, tags)
    if baseline is None:
        return
    commit = git("rev-parse", "--verify", "HEAD^{commit}").decode().strip()
    releases = origin_releases(commit, product, tags)
    pending = owned_fragments(Release(product, version, commit, fragments), releases)
    expected = next_version(name, baseline, (fragment.tier for fragment in pending))
    if version != expected:
        raise ValueError(f"{product}: version {version!r} does not match expected {expected!r} "
                         f"from latest stable origin release {baseline!r} and pending fragments")


def sync():
    releases = origin_releases("HEAD")
    updates = []
    for product in sorted({release.product for release in releases}):
        path = Path(product) / "CHANGELOG.md"
        if has_symlink(path):
            raise ValueError(f"{path}: symlinks are not allowed")
        history = path.read_bytes() if path.exists() else b""
        compiled, consumed = compile_releases(history, [item for item in releases if item.product == product])
        updates.append((path, history, compiled, consumed))
    for path, history, compiled, consumed in updates:
        if compiled != history:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(compiled)
        for source, fragment in consumed:
            if deleted_between(fragment.path, source, "HEAD"):
                continue
            current = Path(fragment.path)
            if has_symlink(current) or not current.is_file():
                continue
            if current.read_bytes() == fragment.data:
                current.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("projects")
    validate_parser = commands.add_parser("validate")
    validate_parser.add_argument("product")
    render_parser = commands.add_parser("render")
    render_parser.add_argument("product")
    render_parser.add_argument("version")
    render_parser.add_argument("--ref")
    version_parser = commands.add_parser("version")
    version_parser.add_argument("product")
    check_parser = commands.add_parser("check-version")
    check_parser.add_argument("product")
    check_parser.add_argument("version", nargs="?")
    commands.add_parser("sync")
    args = parser.parse_args()
    try:
        if args.command == "projects":
            print(" ".join(MANIFESTS))
        elif args.command == "validate":
            working_fragments(product_path(args.product))
        elif args.command == "version":
            print(manifest_version(Path.cwd(), Path(product_path(args.product)).name))
        elif args.command == "check-version":
            check_manifest_version(product_path(args.product), args.version)
        elif args.command == "render":
            sys.stdout.buffer.write(render(product_path(args.product), args.version, args.ref))
        else:
            sync()
    except (ValueError, OSError, UnicodeError, subprocess.CalledProcessError) as error:
        detail = error.stderr.decode().strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        parser.exit(1, f"changelog: {detail}\n")


if __name__ == "__main__":
    main()
