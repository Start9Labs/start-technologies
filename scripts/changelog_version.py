"""Product release baselines, impact tiers and canonical manifest versions."""

import json
from pathlib import Path
import re
from typing import Iterable, Optional


MANIFESTS = {
    "start-os": "package.json",
    "start-sdk": "projects/start-sdk/package.json",
    "start-cli": "projects/start-cli/Cargo.toml",
    "start-tunnel": "projects/start-tunnel/Cargo.toml",
    "start-registry": "projects/start-registry/Cargo.toml",
    "start-wrt": "projects/start-wrt/backend/ctrl/Cargo.toml",
}
TIERS = ("patch", "minor", "major")
NUMBER = r"(?:0|[1-9][0-9]*)"
PRERELEASE_ID = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SUFFIX = (
    r"(?:-(?P<prerelease>" + PRERELEASE_ID + r"(?:\." + PRERELEASE_ID + r")*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)
SEMVER = re.compile(r"(?P<core>" + NUMBER + r"(?:\." + NUMBER + r"){2})" + SUFFIX)
STARTOS_VERSION = re.compile(
    r"(?P<core>0(?:\." + NUMBER + r"){2,3})" + SUFFIX
)


def parse_version(project_name: str, version: str):
    """Return numeric release components and the optional prerelease label."""
    if project_name not in MANIFESTS:
        raise ValueError(f"unknown release project: {project_name!r}")
    pattern = STARTOS_VERSION if project_name == "start-os" else SEMVER
    match = pattern.fullmatch(version) if isinstance(version, str) else None
    if match is None:
        raise ValueError(f"invalid {project_name} version: {version!r}")
    numbers = tuple(int(part) for part in match["core"].split("."))
    if project_name == "start-os":
        numbers = numbers[1:]
        if len(numbers) == 2:
            numbers += (0,)
    return numbers, match["prerelease"]


def next_version(project_name: str, released_version: str, tiers: Iterable[str]) -> str:
    """Apply the highest fragment tier to a stable release; empty tiers preserve it."""
    (major, minor, patch), prerelease = parse_version(project_name, released_version)
    if prerelease is not None:
        raise ValueError(f"release baseline must be stable: {released_version!r}")
    impact = -1
    for tier in tiers:
        if tier not in TIERS:
            raise ValueError(f"invalid fragment tier: {tier!r}")
        impact = max(impact, TIERS.index(tier))
    if impact == -1:
        return released_version
    if impact == 2:
        major, minor, patch = major + 1, 0, 0
    elif impact == 1:
        minor, patch = minor + 1, 0
    else:
        patch += 1
    if project_name == "start-os":
        return f"0.{major}.{minor}" + (f".{patch}" if patch else "")
    return f"{major}.{minor}.{patch}"


def latest_release(project_name: str, tag_names: Iterable[str]) -> Optional[str]:
    """Return the highest stable version in the exact product/v tag namespace."""
    if project_name not in MANIFESTS:
        raise ValueError(f"unknown release project: {project_name!r}")
    prefix = f"{project_name}/v"
    latest = None
    latest_numbers = None
    for tag in tag_names:
        if not tag.startswith(prefix):
            continue
        version = tag[len(prefix):]
        try:
            numbers, prerelease = parse_version(project_name, version)
        except ValueError:
            continue
        if prerelease is None and (latest_numbers is None or numbers > latest_numbers):
            latest, latest_numbers = version, numbers
    return latest


def manifest_version(repo_root: Path, project_name: str) -> str:
    """Read the canonical product version, including prerelease labels."""
    if project_name not in MANIFESTS:
        raise ValueError(f"unknown release project: {project_name!r}")
    path = repo_root / MANIFESTS[project_name]
    text = path.read_text(encoding="utf-8")
    if path.suffix == ".json":
        manifest = json.loads(text)
        version = manifest.get("version") if isinstance(manifest, dict) else None
    else:
        in_package = False
        versions = []
        for line in text.splitlines():
            if line.lstrip().startswith("["):
                in_package = re.fullmatch(r"\s*\[\s*package\s*\]\s*(?:#.*)?", line) is not None
            elif in_package and re.match(r"\s*version\s*(?:=|\.)", line):
                value = re.fullmatch(
                    r'''\s*version\s*=\s*(["'])([^"']*)\1\s*(?:#.*)?''', line
                )
                if value is None:
                    raise ValueError(f"{path}: expected a literal package version")
                versions.append(value[2])
        if len(versions) != 1:
            raise ValueError(f"{path}: expected one [package].version")
        version = versions[0]
    parse_version(project_name, version)
    return version
