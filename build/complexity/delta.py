#!/usr/bin/env python3
"""Prints the complexity delta between two census JSON dumps."""
import json, sys

base, head, ref = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])), sys.argv[3]
key = lambda r: (r['file'], r['name'], r['sloc'] if r['name'] == '<anonymous>' else 0)
B = {key(r): r for r in base['functions']}
H = {key(r): r for r in head['functions']}
tb, th = base['totals'], head['totals']

print(f"Complexity vs {ref[:10]}")
for label, k in (('functions', 'functions'), ('cognitive', 'cognitive'),
                 ('cyclomatic', 'cyclomatic'), ('sloc', 'sloc'),
                 ('fns over 25', 'over25')):
    print(f"  {label:<12}{tb[k]:>7} -> {th[k]:>7}   {th[k]-tb[k]:+d}")

new = sorted((r for k, r in H.items() if k not in B), key=lambda r: -r['cognitive'])
big = [r for r in new if r['cognitive'] > 10]
if big:
    print(f"\n  new functions over cognitive 10 ({len(big)} of {len(new)} new):")
    for r in big[:10]:
        print(f"    cog {r['cognitive']:>4}  {r['name']}  {r['file']}:{r['line']}")

worse = sorted(((H[k], B[k]['cognitive']) for k in H
                if k in B and H[k]['cognitive'] > B[k]['cognitive']),
               key=lambda x: -(x[0]['cognitive'] - x[1]))
if worse:
    print(f"\n  existing functions made more complex ({len(worse)}):")
    for r, old in worse[:10]:
        flag = '  <-- already over 25' if old > 25 else ('  <-- now over 25' if r['cognitive'] > 25 else '')
        print(f"    cog {old} -> {r['cognitive']}  {r['name']}  {r['file']}:{r['line']}{flag}")

better = sorted(((H[k], B[k]['cognitive']) for k in H
                 if k in B and H[k]['cognitive'] < B[k]['cognitive']),
                key=lambda x: x[0]['cognitive'] - x[1])
if better:
    print(f"\n  simplified ({len(better)}):")
    for r, old in better[:5]:
        print(f"    cog {old} -> {r['cognitive']}  {r['name']}  {r['file']}:{r['line']}")

gone = [r for k, r in B.items() if k not in H]
if gone:
    print(f"\n  removed: {len(gone)} functions, {sum(r['cognitive'] for r in gone)} cognitive")

# A helper whose only caller shares its file is the shape a shredded function takes.
# One that anything else calls is a shared utility, and is not the target here.
# Adoption is the counterweight to a rising number: complexity that moved into a
# shared helper a second subsystem now calls reads differently from complexity added.
adopted = []
for k, r in H.items():
    before = set((B[k].get('scopes') or []) if k in B else ())
    after = set(r.get('scopes') or [])
    # Two distinct subsystems is where generality stops being a claim; the first
    # caller is just the author, so creating a utility earns nothing.
    if after - before and len(after) >= 2:
        adopted.append((r, sorted(after - before), len(after)))
if adopted:
    print(f"\n  utilities a second subsystem now depends on ({len(adopted)}):")
    for r, gained, total in sorted(adopted, key=lambda x: -len(x[1]))[:8]:
        print(f"    {r['name']}  +{', '.join(gained)}  (now {total})  {r['file']}")

util_new = [r for r in new if r.get('util')]
if util_new:
    print(f"  of the new functions, {len(util_new)} sit in util modules"
          f" ({sum(r['cognitive'] for r in util_new)} cognitive)")

private = [r for r in new
           if r.get('callers') == 1 and not r.get('shared')
           and not r.get('util') and r['name'] != '<anonymous>']
if private:
    print(f"  single-use helpers alongside their only caller: {len(private)}")

