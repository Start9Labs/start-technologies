#!/usr/bin/env bash
# Regenerate backend/ctrl/assets/oui.tsv.gz from the IEEE MAC-address
# registries (MA-L 24-bit, MA-M 28-bit, MA-S 36-bit).
#
# The IEEE Registration Authority is the canonical source for MAC prefix
# assignments. The registry is append-mostly: existing assignments are
# essentially never changed or reclaimed, only new ones added (a few thousand
# per year, overwhelmingly small IoT vendors), so a checked-in snapshot never
# goes wrong — it only slowly develops misses on brand-new vendors. Re-run
# this script opportunistically (e.g. alongside a release) and commit the
# refreshed asset.
#
# Output format (consumed by backend/ctrl/src/device_ident.rs): gzipped TSV,
# one `HEXPREFIX<TAB>VENDOR` line per assignment, uppercase hex, no separators.
# Prefix length distinguishes the registry: 6 nibbles = MA-L, 7 = MA-M,
# 9 = MA-S. Vendor names are cleaned for display here (corporate suffixes
# stripped) so the Rust side just looks up and formats.
#
# Usage: ./gen-oui-table.sh
#   OUI_CSV_DIR=<dir>  use pre-downloaded oui.csv/mam.csv/oui36.csv instead
#                      of fetching from standards-oui.ieee.org

set -euo pipefail

cd "$(dirname "$0")"
OUT="../backend/ctrl/assets/oui.tsv.gz"

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

if [ -n "${OUI_CSV_DIR:-}" ]; then
  cp "$OUI_CSV_DIR"/oui.csv "$OUI_CSV_DIR"/mam.csv "$OUI_CSV_DIR"/oui36.csv "$workdir"/
else
  echo "Downloading IEEE registries..."
  curl -fsS -o "$workdir/oui.csv" https://standards-oui.ieee.org/oui/oui.csv
  curl -fsS -o "$workdir/mam.csv" https://standards-oui.ieee.org/oui28/mam.csv
  curl -fsS -o "$workdir/oui36.csv" https://standards-oui.ieee.org/oui36/oui36.csv
fi

python3 - "$workdir" <<'EOF' > "$workdir/oui.tsv"
import csv, re, sys

# Corporate-form suffixes stripped (repeatedly) from the tail of org names for
# display. Conservative: only legal-entity forms, never words that can be a
# meaningful part of a brand ("Technologies", "Electronics" stay).
SUFFIX = re.compile(
    r"[\s,.]+(?:inc|incorporated|corp|corporation|company|co|ltd|limited|llc"
    r"|gmbh|ag|bv|b\.v|nv|n\.v|sa|s\.a|sas|srl|s\.r\.l|spa|s\.p\.a"
    r"|oy|ab|a/s|as|aps|kg|kft|pty|pte|pvt|plc|zrt|d\.o\.o|ooo|jsc)[.\s]*$",
    re.IGNORECASE,
)
# Rows that identify no vendor: unallocated-block parents and anonymized
# registrations. Matching one of these must NOT produce a label — exclusion
# here makes the lookup fall through to the next rung instead.
EXCLUDE = {"ieee registration authority", "private"}

def clean(name: str) -> str:
    name = " ".join(name.split())
    while True:
        stripped = SUFFIX.sub("", name).rstrip(" ,.")
        if stripped == name or not stripped:
            return name
        name = stripped

for fname in ("oui.csv", "mam.csv", "oui36.csv"):
    with open(f"{sys.argv[1]}/{fname}", newline="", encoding="utf-8-sig") as f:
        for row in csv.DictReader(f):
            prefix = row["Assignment"].strip().upper()
            org = row["Organization Name"].strip()
            if not prefix or org.lower() in EXCLUDE:
                continue
            org = clean(org)
            if org:
                print(f"{prefix}\t{org}")
EOF

sort -u "$workdir/oui.tsv" | gzip -9n > "$OUT"
echo "Wrote $OUT: $(zcat "$OUT" | wc -l) entries, $(du -h "$OUT" | cut -f1)"
