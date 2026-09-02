#!/usr/bin/env python3
from pathlib import Path
import re

WORKFLOWS = Path('.github/workflows')
RETARGET = WORKFLOWS / 'pr-retarget.yml'
CONFLICT = WORKFLOWS / 'pr-retarget-conflict.yml'
TARGETS = {
    'start-cli.yaml',
    'start-registry.yaml',
    'start-tunnel.yaml',
    'start-wrt.yaml',
    'startos-iso.yaml',
    'test.yaml',
}
EXPECTED_PULL_REQUEST_WORKFLOWS = {
    'conflict-markers.yml',
    'start-cli.yaml',
    'start-registry.yaml',
    'start-tunnel.yaml',
    'start-wrt.yaml',
    'startos-iso.yaml',
    'test.yaml',
}


def block(source: str, heading: str, indent: int = 0) -> str:
    lines = source.splitlines()
    prefix = ' ' * indent
    child_prefix = ' ' * (indent + 2)
    start = lines.index(f'{prefix}{heading}:') + 1
    end = start
    while end < len(lines) and (not lines[end] or lines[end].startswith(child_prefix)):
        end += 1
    return '\n'.join(lines[start:end])


def concurrency_group(source: str) -> str:
    concurrency = block(source, 'concurrency')
    match = re.search(r'^  group: (.+)$', concurrency, re.MULTILINE)
    assert match
    return match.group(1)


def pull_request_workflows() -> set[str]:
    workflows = set()
    for path in WORKFLOWS.glob('*.*ml'):
        trigger = path.read_text().split('\npermissions:', 1)[0]
        if re.search(r'^  pull_request:\s*$', trigger, re.MULTILINE):
            workflows.add(path.name)
    return workflows


def job_permissions(source: str, job: str) -> set[tuple[str, str]]:
    body = block(source.split('\njobs:', 1)[1], job, 2)
    if not re.search(r'^    permissions:\s*$', body, re.MULTILINE):
        return set()
    return set(
        re.findall(r'^      ([\w-]+): (\w+)$', block(body, 'permissions', 4), re.MULTILINE)
    )


listener = RETARGET.read_text()
listener_trigger = listener.split('\npermissions:', 1)[0]
assert 'types: [edited]' in listener_trigger
assert not re.search(
    r'^\s+(?:branches|paths)(?:-ignore)?:', listener_trigger, re.MULTILINE
)
assert "github.event.changes.base && 'base' || 'metadata'" in listener
assert re.search(r'^permissions: \{\}$', listener, re.MULTILINE)

pull_request_targets = pull_request_workflows() - {RETARGET.name}
assert pull_request_targets == EXPECTED_PULL_REQUEST_WORKFLOWS, (
    pull_request_targets,
    EXPECTED_PULL_REQUEST_WORKFLOWS,
)

marker_source = (WORKFLOWS / 'conflict-markers.yml').read_text()
marker_trigger = marker_source.split('\npermissions:', 1)[0]
assert 'types: [opened, synchronize, reopened, edited]' in marker_trigger
assert not re.search(r'^  workflow_call:\s*$', marker_trigger, re.MULTILINE)

groups = {}
for filename in TARGETS:
    source = (WORKFLOWS / filename).read_text()
    trigger = source.split('\npermissions:', 1)[0]
    assert re.search(r'^  workflow_call:\s*$', trigger, re.MULTILINE), filename
    pull_request = block(trigger, 'pull_request', 2)
    assert 'edited' not in pull_request, filename
    job = filename.removesuffix('.yml').removesuffix('.yaml')
    body = block(listener.split('\njobs:', 1)[1], job, 2)
    assert re.search(r'^    if: github\.event\.changes\.base$', body, re.MULTILINE), filename
    assert re.search(
        rf'^    uses: \./\.github/workflows/{re.escape(filename)}$', body, re.MULTILINE
    ), filename
    groups[filename] = concurrency_group(source)

routed = set(re.findall(r'^    uses: \./\.github/workflows/(.+)$', listener, re.MULTILINE))
assert routed == TARGETS, (routed, TARGETS)
assert all('github.workflow' not in group for group in groups.values())
assert len(set(groups.values())) == len(groups), groups

expected_call_permissions = {
    'test': {('contents', 'read'), ('pull-requests', 'read')},
    'start-cli': {('contents', 'read')},
    'start-registry': {('contents', 'read'), ('packages', 'write')},
    'start-tunnel': {('contents', 'read')},
    'start-wrt': {('contents', 'read')},
    'startos-iso': {('contents', 'read'), ('pull-requests', 'read')},
}
for job, expected in expected_call_permissions.items():
    assert job_permissions(listener, job) == expected, job

registry = (WORKFLOWS / 'start-registry.yaml').read_text()
assert job_permissions(registry, 'compile') == {('contents', 'read')}
assert job_permissions(registry, 'create-image') == {
    ('contents', 'read'),
    ('packages', 'write'),
}

conflict = CONFLICT.read_text()
conflict_trigger = conflict.split('\npermissions:', 1)[0]
assert re.search(r'^  pull_request_target:\s*$', conflict_trigger, re.MULTILINE)
assert 'types: [edited]' in conflict_trigger
assert not re.search(
    r'^\s+(?:branches|paths)(?:-ignore)?:', conflict_trigger, re.MULTILINE
)
permissions = block(conflict, 'permissions')
assert set(re.findall(r'^  ([\w-]+): (\w+)$', permissions, re.MULTILINE)) == {
    ('pull-requests', 'read'),
    ('statuses', 'write'),
}
assert "github.event.changes.base && 'base' || 'metadata'" in concurrency_group(conflict)
assert "github.event.changes.base && format('Mergeability (#{0})'" in conflict
assert "|| format('Metadata edit (#{0})'" in conflict
assert 'if: github.event.changes.base' not in conflict
assert 'BASE_CHANGED: ${{ github.event.changes.base != null }}' in conflict
assert 'if [ "$BASE_CHANGED" != true ]; then' in conflict
assert 'github.event.pull_request.base.sha' in conflict
assert 'github.event.pull_request.head.sha' in conflict
assert 'actions/checkout' not in conflict
assert not re.search(r'^\s+uses:', conflict, re.MULTILINE)
assert not re.search(r'\bsecrets\.', conflict)
assert 'pull_request=$(gh api "repos/$GITHUB_REPOSITORY/pulls/$PR_NUMBER")' in conflict
assert 'current_base=$(jq -r \'.base.sha\' <<<"$pull_request")' in conflict
assert 'current_head=$(jq -r \'.head.sha\' <<<"$pull_request")' in conflict
assert 'mergeable=$(jq -r \'.mergeable\' <<<"$pull_request")' in conflict
assert '[ "$current_base" != "$BASE_SHA" ] || [ "$current_head" != "$HEAD_SHA" ]' in conflict
assert "echo 'Pull request metadata changed.'" in conflict
assert 'context="Mergeability (#$PR_NUMBER)"' in conflict
assert 'statuses/$HEAD_SHA' in conflict
assert '-f state=pending' in conflict
assert '-f state=success' in conflict
assert '-f state=failure' in conflict
assert '-f state=error' in conflict
assert 'exit 0' in conflict
assert 'exit 1' in conflict

marker_job = block(marker_source.split('\njobs:', 1)[1], 'conflict-markers', 2)
assert 'name: Conflict Markers' in marker_job
assert 'repository: ${{ github.event.pull_request.head.repo.full_name }}' in marker_job
assert 'ref: ${{ github.event.pull_request.head.sha }}' in marker_job
assert 'persist-credentials: false' in marker_job
assert 'BASE_REPOSITORY_URL: ${{ github.event.pull_request.base.repo.clone_url }}' in marker_job
assert '[[ "$BASE" =~ ^[0-9a-f]{40}$ ]]' in marker_job
base_fetch = 'git fetch --no-tags -- "$BASE_REPOSITORY_URL" "$BASE"'
base_verification = 'test "$(git rev-parse --verify \'FETCH_HEAD^{commit}\')" = "$BASE"'
marker_diff = 'git diff -z --name-only --no-renames "$BASE"...HEAD'
assert base_fetch in marker_job
assert base_verification in marker_job
assert marker_diff in marker_job
assert marker_job.index(base_fetch) < marker_job.index(base_verification) < marker_job.index(
    marker_diff
)
assert 'No conflict markers in the files this pull request changes.' in marker_job

print(
    f'Validated the explicit {len(TARGETS)}-workflow retarget call set, direct marker '
    'rescans, isolated package publication, and conflict-listener isolation.'
)
