#!/bin/bash
set -e

cd "$(dirname "$0")"
ROOT="$(pwd)"

# Build output (gitignored). Deploy rsyncs from docs/<book>/<version>.
OUT="$ROOT/docs"
rm -rf "$OUT"
mkdir -p "$OUT"

# Books live in their product dirs now; map book name -> source dir.
book_dir() {
  case "$1" in
    start-os) echo "$ROOT/../start-os/docs" ;;
    start-tunnel) echo "$ROOT/../start-tunnel/docs" ;;
    packaging) echo "$ROOT/../start-sdk/docs" ;;
    start-wrt) echo "$ROOT/../start-wrt/docs" ;;
    *) echo "$ROOT/$1" ;;
  esac
}

# Build each book listed in versions.conf
while IFS='=' read -r book version; do
  [[ -z "$book" || "$book" =~ ^# ]] && continue

  (cd "$(book_dir "$book")" && MDBOOK_OUTPUT__HTML__SITE_URL="/$book/$version/" \
    mdbook build -d "$OUT/$book/$version")

  # Redirect stub: /book/ → /book/version/
  cat > "$OUT/$book/index.html" <<EOF
<!doctype html><meta http-equiv="refresh" content="0; url=/$book/$version/">
EOF
done < versions.conf

# Landing page
cp landing/index.html "$OUT/index.html"

echo "Build complete: $OUT"
