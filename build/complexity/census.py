#!/usr/bin/env python3
"""Cognitive-complexity census over the tree. Emits JSON, or a table."""
import argparse, collections, json, os, re, subprocess, sys, tempfile

CFG_TEST = re.compile(r'#\[cfg\(test\)\]')
MACRO_RULES = re.compile(r'macro_rules!\s*\w+\s*\{')
CALL_SITE = re.compile(r'\b([A-Za-z_]\w*)\s*[(:<]')
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

def macro_body_lines(src):
    """Lines inside `macro_rules!` bodies. Control flow there is invisible to the metrics."""
    total = 0
    for m in MACRO_RULES.finditer(src):
        depth, k, n = 0, src.index('{', m.start()), len(src)
        start = k
        while k < n:
            if src[k] == '{': depth += 1
            elif src[k] == '}':
                depth -= 1
                if depth == 0: break
            k += 1
        total += src.count('\n', start, k)
    return total


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

def own(node, metric):
    """A space's own score. The parser's `sum` folds in every nested space."""
    total = node.get('metrics', {}).get(metric, {}).get('sum')
    if total is None:
        return None
    nested = sum((c.get('metrics', {}).get(metric, {}).get('sum') or 0)
                 for c in node.get('spaces', []))
    return max(0, int(total - nested))


def collect(node, rel, out, is_root=True):
    # The outermost space is the file, not a function.
    if node.get('kind') == 'function' and not is_root:
        cog = own(node, 'cognitive')
        if cog is not None:
            name = node.get('name') or ''
            if not name or os.sep in name:
                name = '<anonymous>'
            out.append({'file': rel, 'name': name,
                        'line': node.get('start_line'), 'cognitive': cog,
                        'cyclomatic': own(node, 'cyclomatic') or 0,
                        'sloc': int(node.get('metrics', {}).get('loc', {}).get('sloc') or 0)})
    for c in node.get('spaces', []):
        collect(c, rel, out, False)

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
    files = macro_lines = 0
    for src, rel in sources(root, scopes):
        try: text = open(src, encoding='utf-8', errors='replace').read()
        except OSError: continue
        if rel.endswith('.rs'):
            text = strip_rust_tests(text)
            macro_lines += macro_body_lines(text)
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
    return rows, macro_lines

def count_callers(rows, root, scopes):
    """Call sites for each function, counted across the tree. The definition is not one."""
    used = collections.Counter()
    for src, _ in sources(root, scopes):
        try: text = open(src, encoding='utf-8', errors='replace').read()
        except OSError: continue
        used.update(CALL_SITE.findall(text))
    defs = collections.Counter(r['name'] for r in rows)
    for r in rows:
        name = r['name']
        if name == '<anonymous>':
            continue
        r['callers'] = max(0, used[name] - defs[name])


def totals(rows, macro_lines):
    return {'functions': len(rows),
            'cognitive': sum(r['cognitive'] for r in rows),
            'cyclomatic': sum(r['cyclomatic'] for r in rows),
            'sloc': sum(r['sloc'] for r in rows),
            'macro_lines': macro_lines,
            'over25': sum(1 for r in rows if r['cognitive'] > 25)}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--root', default='.')
    ap.add_argument('--rca', default=os.environ.get('RCA', 'rust-code-analysis-cli'))
    ap.add_argument('--scope', action='append')
    ap.add_argument('--json', action='store_true')
    ap.add_argument('--top', type=int, default=25)
    a = ap.parse_args()
    scopes = a.scope or ['shared-libs', 'projects']
    rows, macro_lines = census(a.root, scopes, a.rca)
    if a.json:
        count_callers(rows, a.root, scopes)
        json.dump({'totals': totals(rows, macro_lines), 'functions': rows}, sys.stdout, sort_keys=True)
        return
    t = totals(rows, macro_lines)
    print(f"functions {t['functions']}  cognitive {t['cognitive']}  cyclomatic {t['cyclomatic']}  "
          f"sloc {t['sloc']}  macro-lines {t['macro_lines']}  over25 {t['over25']}")
    for r in sorted(rows, key=lambda r: -r['cognitive'])[:a.top]:
        print(f"  {r['cognitive']:>5} {r['sloc']:>5}  {r['name'][:34]:<34} {r['file']}:{r['line']}")

main()
