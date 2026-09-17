#!/bin/bash
# Publishes a directory as a folder in the NextExplorer volume assigned to the
# account, replacing the folder's previous contents in one rename. Named entries
# of the previous contents are carried over.
#
#   NEXTEXPLORER_URL=https://… NEXTEXPLORER_EMAIL=… NEXTEXPLORER_PASSWORD=… \
#     publish.sh <dir> <folder> [entry-to-keep…]
set -euo pipefail

[ $# -ge 2 ] || { echo "usage: $0 <dir> <folder> [entry-to-keep…]" >&2; exit 2; }
: "${NEXTEXPLORER_URL:?}" "${NEXTEXPLORER_EMAIL:?}" "${NEXTEXPLORER_PASSWORD:?}"
src=$1
folder=$2
shift 2
base="${NEXTEXPLORER_URL%/}/api"

jar=$(mktemp)
trap 'rm -f "$jar"' EXIT

api() {
  local method=$1 path=$2 body
  shift 2
  body=$(curl -sS --fail-with-body -X "$method" -b "$jar" -c "$jar" "$base$path" "$@") \
    || { echo "$method $path: $body" >&2; return 1; }
  printf '%s' "$body"
}
json() { api "$1" "$2" -H 'content-type: application/json' --data-binary @-; }
exists() {
  api GET "/browse/$(jq -rn --arg s "$1" '$s | split("/") | map(@uri) | join("/")')" \
    | jq -e --arg n "$2" 'any(.items[]; .name == $n)' > /dev/null
}
rename() {
  jq -n --arg path "$1" --arg name "$2" --arg newName "$3" '{$path, $name, $newName}' \
    | json POST /files/rename > /dev/null
}

jq -n --arg email "$NEXTEXPLORER_EMAIL" --arg password "$NEXTEXPLORER_PASSWORD" '{$email, $password}' \
  | json POST /auth/login > /dev/null

# The volume's label is the root of every path the account can address.
volume=$(api GET /volumes | jq -er '
  if length != 1 then error("the account must have exactly one assigned volume, found \(length)")
  elif .[0].accessMode != "readwrite" then error("the assigned volume is read-only")
  else .[0].path end')

case $folder in
  */*) parent="$volume/${folder%/*}" ;;
  *) parent=$volume ;;
esac
name=${folder##*/}
new="$name.new"
old="$name.old"

# A run that died between its two renames left the live contents under .old.
if ! exists "$parent" "$name" && exists "$parent" "$old"; then
  rename "$parent" "$old" "$name"
fi
jq -n --arg path "$parent" --arg new "$new" --arg old "$old" '{items: [{$path, name: $new}, {$path, name: $old}]}' \
  | json DELETE /files > /dev/null
jq -n --arg path "$parent" --arg name "$new" '{$path, $name}' | json POST /files/folder > /dev/null

# An upload never overwrites: a name already in use gets " (1)" appended.
# The text fields must precede the file; multer exposes only the fields read before it.
count=0
while IFS= read -r -d '' rel; do
  api POST /upload --form-string "uploadTo=$parent/$new" --form-string "relativePath=$rel" \
    -F "filedata=@$src/$rel" > /dev/null
  count=$((count + 1))
done < <(find "$src" -type f -printf '%P\0')

for entry in "$@"; do
  if ! exists "$parent" "$name" || ! exists "$parent/$name" "$entry"; then
    echo "::warning::$folder/$entry is not there to keep"
    continue
  fi
  to=$(jq -n --arg path "$parent/$name" --arg name "$entry" --arg destination "$parent/$new" \
    '{items: [{$path, $name}], $destination}' | json POST /files/copy | jq -r '.items[0].to')
  [ "$to" = "$parent/$new/$entry" ] || { echo "$folder/$entry collides with the published contents" >&2; exit 1; }
done

if exists "$parent" "$name"; then
  rename "$parent" "$name" "$old"
fi
rename "$parent" "$new" "$name"
jq -n --arg path "$parent" --arg name "$old" '{items: [{$path, $name}]}' | json DELETE /files > /dev/null

api POST /auth/logout > /dev/null
echo "published $count files to $parent/$name"
