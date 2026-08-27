#!/usr/bin/env python3
"""Per-function cognitive complexity for the tree, as JSON or a table."""
import argparse, collections, json, os, re, subprocess, sys, tempfile

SCOPES = ['shared-libs', 'projects']
EXCLUDE = ('/node_modules/', '/dist/', '/target/', '/.angular/', '/out-tsc/',
           '/osBindings/', '/locales/', '/__snapshots__/', '/__fixtures__/',
           '/patch-db/client/', '/exver/exver.ts')
CALL_SITE = re.compile(r'\b([A-Za-z_]\w*)\s*[(:<]')


def kept(path):
    return (path.endswith(('.rs', '.ts', '.js'))
            and not path.endswith(('.spec.ts', '.d.ts'))
            and not any(x in '/' + path for x in EXCLUDE))


def spaces(node, root=True):
    if node.get('kind') == 'function' and not root:
        yield node
    for child in node.get('spaces', []):
        yield from spaces(child, False)


def census(root, scopes, bca):
    out = tempfile.mkdtemp(prefix='cx-')
    run = subprocess.run([bca, 'metrics', '-O', 'json', '--exclude-tests', '--output-dir', out,
                          *(os.path.join(root, s) for s in scopes)],
                         capture_output=True, text=True)
    if run.returncode != 0:
        sys.exit(f"complexity: {bca} failed ({run.returncode}): {run.stderr.strip().splitlines()[-1] if run.stderr.strip() else 'no output'}")
    rows = []
    for dirpath, _, names in os.walk(out):
        for name in names:
            if not name.endswith('.json'):
                continue
            try:
                doc = json.load(open(os.path.join(dirpath, name)))
            except (OSError, ValueError):
                continue
            rel = os.path.relpath(doc.get('name', ''), root)
            if not kept(rel):
                continue
            for fn in spaces(doc):
                m = fn.get('metrics', {})
                label = fn.get('name') or ''
                rows.append({
                    'file': rel,
                    'name': '<anonymous>' if not label or os.sep in label else label,
                    'line': fn.get('start_line'),
                    'cognitive': int(m.get('cognitive', {}).get('value') or 0),
                    'cyclomatic': int(m.get('cyclomatic', {}).get('value') or 0),
                    'sloc': int(m.get('loc', {}).get('sloc') or 0),
                })
    return rows


def count_callers(rows, root, scopes):
    """Call sites per function name across the tree. A definition is not a call site."""
    used = collections.Counter()
    for scope in scopes:
        for dirpath, dirs, names in os.walk(os.path.join(root, scope)):
            dirs[:] = [d for d in dirs if d not in
                       ('node_modules', 'target', 'dist', '.angular', 'out-tsc', 'osBindings', 'locales')]
            for name in names:
                path = os.path.join(dirpath, name)
                if not kept(os.path.relpath(path, root)):
                    continue
                try:
                    used.update(CALL_SITE.findall(open(path, encoding='utf-8', errors='replace').read()))
                except OSError:
                    pass
    defined = collections.Counter(r['name'] for r in rows)
    for r in rows:
        if r['name'] != '<anonymous>':
            r['callers'] = max(0, used[r['name']] - defined[r['name']])


def assert_parsed(bca, root, scopes, rows):
    """Grammar rot is silent: a file the parser cannot read yields no functions, not an error."""
    if not rows:
        sys.exit('complexity: the census found no functions at all — the analyzer did not run')
    out = subprocess.run([bca, 'count', '--type', 'ERROR', *(os.path.join(root, s) for s in scopes)],
                         capture_output=True, text=True).stdout
    total = found = 0
    for line in out.splitlines():
        digits = line.split(':')[-1].strip().replace(',', '')
        if line.startswith('Total nodes'):
            total = int(digits or 0)
        elif line.startswith('Found nodes'):
            found = int(digits or 0)
    if total and found / total > 0.005:
        sys.exit(f"complexity: {found / total:.3%} of AST nodes are parse errors — "
                 "the grammar has fallen behind the language")


def totals(rows):
    return {'functions': len(rows),
            'cognitive': sum(r['cognitive'] for r in rows),
            'cyclomatic': sum(r['cyclomatic'] for r in rows),
            'sloc': sum(r['sloc'] for r in rows),
            'over25': sum(1 for r in rows if r['cognitive'] > 25)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', default='.')
    ap.add_argument('--bca', default=os.environ.get('BCA', 'bca'))
    ap.add_argument('--scope', action='append')
    ap.add_argument('--json', action='store_true')
    ap.add_argument('--top', type=int, default=25)
    a = ap.parse_args()
    scopes = a.scope or SCOPES
    rows = census(a.root, scopes, a.bca)
    assert_parsed(a.bca, a.root, scopes, rows)
    if a.json:
        count_callers(rows, a.root, scopes)
        json.dump({'totals': totals(rows), 'functions': rows}, sys.stdout, sort_keys=True)
        return
    t = totals(rows)
    print(f"functions {t['functions']}  cognitive {t['cognitive']}  cyclomatic {t['cyclomatic']}  "
          f"sloc {t['sloc']}  over25 {t['over25']}")
    for r in sorted(rows, key=lambda r: -r['cognitive'])[:a.top]:
        print(f"  {r['cognitive']:>5} {r['sloc']:>5}  {r['name'][:34]:<34} {r['file']}:{r['line']}")


main()
