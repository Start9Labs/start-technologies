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

# A page that sends the browser on, fragment included.
stub() {
  cat > "$1" <<EOF
<!doctype html><meta http-equiv="refresh" content="0; url=$2"><script>location.replace("$2"+location.hash)</script>
EOF
}

books='{'
# Build each book listed in versions.conf
while IFS='=' read -r book version; do
  [[ -z "$book" || "$book" =~ ^# ]] && continue

  (cd "$(book_dir "$book")" && MDBOOK_OUTPUT__HTML__SITE_URL="/$book/$version/" \
    mdbook build -d "$OUT/$book/$version")

  # Unversioned URLs go to the current version: /book/ and every /book/page.html
  stub "$OUT/$book/index.html" "/$book/$version/"
  for page in "$OUT/$book/$version"/*.html; do
    page=$(basename "$page")
    [ "$page" = index.html ] || stub "$OUT/$book/$page" "/$book/$version/$page"
  done
  books="$books\"$book\":\"$version\","
done < versions.conf

# Landing page, and the 404 page with the book list its redirects need
cp landing/index.html "$OUT/index.html"
sed "s|/\*BOOKS\*/ {}|${books%,}}|" landing/404.html > "$OUT/404.html"

# llms.txt for the site and for each book
(cd scripts && { [ -d node_modules ] || npm ci; } && npm run generate-llms-txt)

echo "Build complete: $OUT"
