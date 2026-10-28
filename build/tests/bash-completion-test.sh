#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/../.."
completion_main=${BASH_COMPLETION_MAIN:-/usr/share/bash-completion/bash_completion}
[[ -f $completion_main ]] || { echo "bash-completion is required" >&2; exit 1; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

make --no-print-directory -s -f - DESTDIR="$stage" PLATFORM=x86_64 <<'MAKE'
include build/common.mk
.DEFAULT_GOAL := install
install:
	$(call install-bash-completion,start-cli)
	$(call install-bash-completion,start-tunnel)
MAKE

mkdir -p "$stage/bin" "$stage/home"
cat > "$stage/bin/generator" <<'SH'
#!/bin/bash
set -euo pipefail
[[ $* == 'completions bash' ]]
printf '%s\n' "${0##*/}" >> "$CALL_LOG"
printf 'complete -W generated-subcommand %q\n' "${0##*/}"
SH
chmod +x "$stage/bin/generator"
ln -s generator "$stage/bin/start-cli"
ln -s generator "$stage/bin/start-tunnel"

PATH="$stage/bin:$PATH" HOME="$stage/home" XDG_DATA_DIRS="$stage/usr/share" \
    BASH_COMPLETION_USER_DIR="$stage/home" CALL_LOG="$stage/calls" \
    bash --noprofile --norc -s -- "$completion_main" <<'SH'
set -euo pipefail
source "$1"
[[ $(complete -p -D) =~ -F[[:space:]]+([^[:space:]]+) ]]
loader=${BASH_REMATCH[1]}
[[ ! -e $CALL_LOG ]]
for command in start-cli start-tunnel; do
    if complete -p "$command" &>/dev/null; then
        echo "completion for $command loaded eagerly" >&2
        exit 1
    fi
    if "$loader" "$command"; then
        echo "completion loader did not request a retry" >&2
        exit 1
    else
        [[ $? == 124 ]]
    fi
    [[ $(complete -p "$command") == "complete -W 'generated-subcommand' $command" ]]
done
[[ $(< "$CALL_LOG") == $'start-cli\nstart-tunnel' ]]
SH

echo 'Bash completion staging and lazy loading passed'
