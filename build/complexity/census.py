#!/usr/bin/env python3
"""Cognitive-complexity census over the tree. Emits JSON, or a table."""
import argparse, json, os, re, subprocess, sys, tempfile

CFG_TEST = re.compile(r'#\[cfg\(test\)\]')
EXCLUDE = ('/node_modules/', '/dist/', '/target/', '/.angular/', '/out-tsc/',
           '/osBindings/', '/locales/', '/__snapshots__/', '/__fixtures__/',
           '/patch-db/client/', '/exver/exver.ts')

def strip_rust_tests(src):
    """Removes `#[cfg(test)]`-gated items by brace matching. Inline tests are never measured."""
    out, i, n = [], 0, len(src)
    while i < n:
        m = CFG_TEST.search(src, i)
        if not m:
            out.append(src[i:]); break
        out.append(src[i:m.start()])
        j = src.find('{', m.end())
        if j < 0: break
        depth, k = 0, j
        s = ch = cl = cb = False
        while k < n:
            c = src[k]
            if cl:
                if c == '\n': cl = False
            elif cb:
                if src.startswith('*/', k): cb = False; k += 1
            elif s:
                if c == '\\': k += 1
                elif c == '"': s = False
            elif ch:
                if c == '\\': k += 1
                elif c == "'": ch = False
            elif src.startswith('//', k): cl = True; k += 1
            elif src.startswith('/*', k): cb = True; k += 1
            elif c == '"': s = True
            elif c == '{': depth += 1
            elif c == '}':
                depth -= 1
                if depth == 0: k += 1; break
            k += 1
        i = k
    return ''.join(out)

def sources(root, scopes):
    for scope in scopes:
        for dp, dns, fns in os.walk(os.path.join(root, scope)):
            dns[:] = [d for d in dns if d not in
                      ('node_modules', 'target', 'dist', '.angular', 'out-tsc', 'osBindings', 'locales')]
            for fn in fns:
                if not fn.endswith(('.rs', '.ts')) or fn.endswith(('.spec.ts', '.d.ts')):
                    continue
                p = os.path.join(dp, fn)
                rel = os.path.relpath(p, root)
                if any(x in '/' + rel for x in EXCLUDE):
                    continue
                yield p, rel

def collect(node, rel, out):
    if node.get('kind') == 'function':
        m = node.get('metrics', {})
        cog = m.get('cognitive', {}).get('sum')
        if cog is not None:
            name = node.get('name') or ''
            if not name or os.sep in name:
                name = '<anonymous>'
            out.append({'file': rel, 'name': name,
                        'line': node.get('start_line'), 'cognitive': int(cog),
                        'cyclomatic': int(m.get('cyclomatic', {}).get('sum') or 0),
                        'sloc': int(m.get('loc', {}).get('sloc') or 0)})
    for c in node.get('spaces', []):
        collect(c, rel, out)

def error_ratio(rca, path):
    """Share of AST nodes tree-sitter could not parse."""
    out = subprocess.run([rca, '-C', 'ERROR', '-p', path],
                         capture_output=True, text=True).stdout
    total = found = 0
    for line in out.splitlines():
        digits = line.split(':')[-1].strip().replace(',', '')
        if line.startswith('Total nodes'): total = int(digits or 0)
        elif line.startswith('Found nodes'): found = int(digits or 0)
    return (found / total) if total else 0.0


def census(root, scopes, rca):
    staged = tempfile.mkdtemp(prefix='cx-src-')
    outdir = tempfile.mkdtemp(prefix='cx-json-')
    files = 0
    for src, rel in sources(root, scopes):
        try: text = open(src, encoding='utf-8', errors='replace').read()
        except OSError: continue
        if rel.endswith('.rs'):
            text = strip_rust_tests(text)
        dst = os.path.join(staged, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        open(dst, 'w', encoding='utf-8').write(text)
        files += 1
    subprocess.run([rca, '-m', '-p', staged, '-O', 'json', '-o', outdir],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    rows, parsed = [], 0
    for dp, _, fns in os.walk(outdir):
        for fn in fns:
            if not fn.endswith('.json'): continue
            try: d = json.load(open(os.path.join(dp, fn)))
            except Exception: continue
            parsed += 1
            collect(d, os.path.relpath(d.get('name', ''), staged), rows)
    # A file the parser cannot read yields no functions rather than an error.
    if files and parsed / files < 0.98:
        sys.exit(f"complexity: parser read {parsed} of {files} files — refusing to report a partial census")
    bad = error_ratio(rca, staged)
    if bad > 0.005:
        sys.exit(f"complexity: {bad:.3%} of AST nodes are parse errors — the grammar has fallen behind the language")
    return rows

def totals(rows):
    return {'functions': len(rows),
            'cognitive': sum(r['cognitive'] for r in rows),
            'sloc': sum(r['sloc'] for r in rows),
            'over25': sum(1 for r in rows if r['cognitive'] > 25)}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', default='.')
    ap.add_argument('--rca', default=os.environ.get('RCA', 'rust-code-analysis-cli'))
    ap.add_argument('--scope', action='append')
    ap.add_argument('--json', action='store_true')
    ap.add_argument('--top', type=int, default=25)
    a = ap.parse_args()
    rows = census(a.root, a.scope or ['shared-libs', 'projects'], a.rca)
    if a.json:
        json.dump({'totals': totals(rows), 'functions': rows}, sys.stdout, sort_keys=True)
        return
    t = totals(rows)
    print(f"functions {t['functions']}  cognitive {t['cognitive']}  sloc {t['sloc']}  over25 {t['over25']}")
    for r in sorted(rows, key=lambda r: -r['cognitive'])[:a.top]:
        print(f"  {r['cognitive']:>5} {r['sloc']:>5}  {r['name'][:34]:<34} {r['file']}:{r['line']}")

main()
