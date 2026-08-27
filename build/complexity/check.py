#!/usr/bin/env python3
"""Fails when a PR body's Complexity block is absent, unanswered, or disagrees with a fresh census."""
import re, sys

TOTALS = re.compile(
    r'functions\s+(\d+)\s*->\s*(\d+).*?'
    r'cognitive\s+(\d+)\s*->\s*(\d+).*?'
    r'cyclomatic\s+(\d+)\s*->\s*(\d+).*?'
    r'sloc\s+(\d+)\s*->\s*(\d+)', re.S)

# Each question is asked only when the census actually reported the thing it is about.
QUESTIONS = (
    ('simplest alternative', r'[Ss]implest alternative[^\n]*:[^\S\n]*(\S[^\n]*)', None),
    ('existing helper', r'[Ee]xisting helper[^\n]*:[^\S\n]*(\S[^\n]*)', None),
    ('over-25 justification', r'over 25[^\n]*:[^\S\n]*(\S[^\n]*)', '<-- '),
    ('single-call-site helpers', r'one call site[^\n]*:[^\S\n]*(\S[^\n]*)',
     'new functions with one call site ('),
)

NOT_APPLICABLE = {'none', 'none.', 'n/a', 'na', 'nothing', 'not applicable'}


def main(body_path, fresh_path):
    body = open(body_path, encoding='utf-8').read()
    fresh = open(fresh_path, encoding='utf-8').read()
    if '## Complexity' not in body:
        sys.exit("PR body has no '## Complexity' section. Run `make complexity-diff` and paste it.")
    actual, claimed = TOTALS.search(fresh), TOTALS.search(body)
    if not actual:
        sys.exit("internal: could not parse the fresh census")
    if not claimed:
        sys.exit("PR body's Complexity section carries no `make complexity-diff` output.")
    if claimed.groups() != actual.groups():
        c, a = claimed.groups(), actual.groups()
        sys.exit("PR body's complexity numbers do not match a fresh run.\n"
                 f"  body:  functions {c[0]}->{c[1]}  cognitive {c[2]}->{c[3]}  cyclomatic {c[4]}->{c[5]}\n"
                 f"  fresh: functions {a[0]}->{a[1]}  cognitive {a[2]}->{a[3]}  cyclomatic {a[4]}->{a[5]}\n"
                 "Re-run `make complexity-diff` and paste the current output.")
    for label, pat, trigger in QUESTIONS:
        raised = trigger is None or trigger in fresh
        m = re.search(pat, body)
        answer = m.group(1).strip() if m else ''
        if not raised:
            continue
        if answer.lower() in NOT_APPLICABLE and trigger is not None:
            sys.exit(f"PR body answers '{label}' with '{answer}', but the census reported it.")
        if len(answer) < 12:
            sys.exit(f"PR body's Complexity section leaves '{label}' unanswered.")
    print("Complexity block present, current, and answered.")


main(sys.argv[1], sys.argv[2])
