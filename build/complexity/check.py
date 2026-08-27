#!/usr/bin/env python3
"""Fails when a PR body's Complexity block is absent, unanswered, or disagrees with a fresh census."""
import re, sys

TOTALS = re.compile(
    r'functions\s+(\d+)\s*->\s*(\d+).*?'
    r'cognitive\s+(\d+)\s*->\s*(\d+).*?'
    r'sloc\s+(\d+)\s*->\s*(\d+)', re.S)

QUESTIONS = (
    ('simplest alternative', r'[Ss]implest alternative[^\n]*:[^\S\n]*(\S[^\n]*)'),
    ('existing helper', r'[Ee]xisting helper[^\n]*:[^\S\n]*(\S[^\n]*)'),
    ('over-25 justification', r'over 25[^\n]*:[^\S\n]*(\S[^\n]*)'),
)

def main(body_path, fresh_path):
    body = open(body_path, encoding='utf-8').read()
    fresh = open(fresh_path, encoding='utf-8').read()
    if '## Complexity' not in body:
        sys.exit("PR body has no '## Complexity' section. Run `make complexity-diff` and paste it.")
    actual = TOTALS.search(fresh)
    claimed = TOTALS.search(body)
    if not actual:
        sys.exit("internal: could not parse the fresh census")
    if not claimed:
        sys.exit("PR body's Complexity section carries no `make complexity-diff` output.")
    if claimed.groups() != actual.groups():
        c, a = claimed.groups(), actual.groups()
        sys.exit("PR body's complexity numbers do not match a fresh run.\n"
                 f"  body:  functions {c[0]}->{c[1]}  cognitive {c[2]}->{c[3]}  sloc {c[4]}->{c[5]}\n"
                 f"  fresh: functions {a[0]}->{a[1]}  cognitive {a[2]}->{a[3]}  sloc {a[4]}->{a[5]}\n"
                 "Re-run `make complexity-diff` and paste the current output.")
    for label, pat in QUESTIONS:
        m = re.search(pat, body)
        if not m or len(m.group(1).strip()) < 12:
            sys.exit(f"PR body's Complexity section leaves '{label}' unanswered.")
    print("Complexity block present, current, and answered.")

main(sys.argv[1], sys.argv[2])
