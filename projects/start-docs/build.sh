#!/bin/bash
set -e

cd "$(dirname "$0")"
ROOT="$(pwd)"
REPO="$(cd ../.. && pwd)"

# Build output (gitignored). Deploy publishes docs/ whole.
OUT="$ROOT/docs"
# Archived versions' sources, extracted from their release tags (gitignored).
ARCHIVE="$ROOT/.archive"
rm -rf "$OUT" "$ARCHIVE"
mkdir -p "$OUT" "$ARCHIVE"

# A book's source dir, relative to the repo root.
book_path() {
  case "$1" in
    start-os) echo projects/start-os/docs ;;
    start-tunnel) echo projects/start-tunnel/docs ;;
    packaging) echo projects/start-sdk/docs ;;
    start-wrt) echo projects/start-wrt/docs ;;
    *) echo "projects/start-docs/$1" ;;
  esac
}

book_title() {
  sed -n 's/^title *= *"\(.*\)"/\1/p' "$REPO/$(book_path "$1")/book.toml"
}

# A page that sends the browser on, fragment included. The query's `version`
# (sent by the StartOS UI) picks the archived version that holds the page.
stub() {
  cat > "$1" <<EOF
<!doctype html><meta http-equiv="refresh" content="0; url=/$2/$3/$4"><script>var d="$3",a={$5},v=(new URLSearchParams(location.search).get("version")||"").split(/[-+:~]/)[0];for(var k in a)if(v===k||v.indexOf(k+".")===0)d=a[k];location.replace("/$2/"+d+"/$4"+location.hash)</script>
EOF
}

books=()
declare -A current archives tag
while IFS='=' read -r book spec; do
  [[ -z "$book" || "$book" =~ ^# ]] && continue
  [[ -v "current[$book]" ]] || books+=("$book")
  version="${spec%%@*}"
  if [[ "$spec" == *@* ]]; then
    archives[$book]+="$version "
    tag[$book/$version]="${spec#*@}"
  else
    current[$book]="$version"
  fi
done < versions.conf

build_book() { # book version srcdir
  local site="/$1/${2:+$2/}"
  (cd "$3" && MDBOOK_OUTPUT__HTML__SITE_URL="$site" mdbook build -d "$OUT$site")
}

landing_versions=""
archive_sections=""
versions_json=""
for book in "${books[@]}"; do
  cur="${current[$book]}"
  build_book "$book" "$cur" "$REPO/$(book_path "$book")"
  if [ -z "$cur" ]; then
    versions_json+="\"$book\":{\"current\":null,\"archives\":[]},"
    continue
  fi
  versions_json+="\"$book\":{\"current\":\"$cur\",\"archives\":["

  title="$(book_title "$book")"
  landing_versions+="s|<!--version:$book-->|v$cur|;"
  links=""

  for version in ${archives[$book]}; do
    t="${tag[$book/$version]}"
    git -C "$REPO" rev-parse -q --verify "refs/tags/$t" > /dev/null ||
      git -C "$REPO" fetch -q --depth=1 origin "refs/tags/$t:refs/tags/$t"
    mkdir -p "$ARCHIVE/$book/$version"
    git -C "$REPO" archive "$t" "$(book_path "$book")" projects/start-docs/theme |
      tar -x -C "$ARCHIVE/$book/$version"
    build_book "$book" "$version" "$ARCHIVE/$book/$version/$(book_path "$book")"

    for file in $(cd "$OUT/$book/$version" && find . -name '*.html' | sed 's|^\./||'); do
      page="$file"
      [ "$page" = index.html ] && page=""
      [ -e "$OUT/$book/$cur/$file" ] || page=""
      sed -i "0,/<main>/s|<main>|<main><div style=\"margin-bottom:1.5em;padding:.6em 1em;border-left:4px solid var(--links);background:var(--quote-bg)\">These are the docs for $title v$version. <a href=\"/$book/$page\">Go to the current version, v$cur</a></div>|" \
        "$OUT/$book/$version/$file"
    done

    links+="<li><a href=\"/$book/$version/\">v$version</a></li>"
    versions_json+="\"$version\","
  done
  versions_json="${versions_json%,}]},"

  # 0.3.5.x is the retired Sphinx site, seeded on the host by hand.
  [ "$book" = start-os ] && links+='<li><a href="/0.3.5.x/">v0.3.5.x</a></li>'
  [ -n "$links" ] && archive_sections+="<h2>$title</h2><ul>$links</ul>"

  # Unversioned URLs go to the current version, or the newest archive that has the page.
  for page in $(for v in $cur ${archives[$book]}; do (cd "$OUT/$book/$v" && ls *.html); done | sort -u); do
    default="" map=""
    for v in $cur ${archives[$book]}; do
      [ -e "$OUT/$book/$v/$page" ] || continue
      default="${default:-$v}"
      map+="\"${v%.x}\":\"$v\","
    done
    target="$page"
    [ "$page" = index.html ] && target=""
    stub "$OUT/$book/$page" "$book" "$default" "$target" "${map%,}"
  done
done

# Landing page, archives index, 404 page and versions.json
sed "$landing_versions" landing/index.html > "$OUT/index.html"
mkdir -p "$OUT/archives"
sed "s|<!--ARCHIVES-->|$archive_sections|" landing/archives.html > "$OUT/archives/index.html"
echo "{${versions_json%,}}" > "$OUT/versions.json"
sed "s|/\*BOOKS\*/ {}|{${versions_json%,}}|" landing/404.html > "$OUT/404.html"

# llms.txt for the site and for each book
(cd scripts && { [ -d node_modules ] || npm ci; } && npm run generate-llms-txt)

echo "Build complete: $OUT"
