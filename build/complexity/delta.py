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
                 ('sloc', 'sloc'), ('fns over 25', 'over25')):
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
