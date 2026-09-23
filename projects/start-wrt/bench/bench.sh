#!/usr/bin/env bash
# Live-bench driver for a StartWRT test router. See projects/start-wrt/AGENTS.md "Live bench".

set -euo pipefail

WRT_HOST=${WRT_HOST:-wrt-bench}
OS_HOST=${OS_HOST:-os-bench}
WRT_CONSOLE=${WRT_CONSOLE:-/dev/wrt-console}
WRT_CONSOLE_BAUD=${WRT_CONSOLE_BAUD:-115200}
MGMT_PORT=${MGMT_PORT:-2222}
BENCH_STATE_DIR=${BENCH_STATE_DIR:-${XDG_STATE_HOME:-$HOME/.local/state}/startwrt-bench}
CONSOLE_SESSION=wrt-console
CONSOLE_LOG=$BENCH_STATE_DIR/console.log
SNAPSHOT_DIR=$BENCH_STATE_DIR/snapshots

REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)

usage() {
	cat <<'EOF'
Usage: bench.sh <command> [args]

  mgmt install|status|remove        the bench's own SSH port on the router, independent of Remote Access
  preflight [--with-os]             check the bench hosts, the console, and router capabilities;
                                    os-bench fails the check only with --with-os
  deployed                          compare the local startwrt build with the router's binary
  smoke                             run the standing regression checks (LAN client, UI both sides, log)
  wait-ssh [timeout]                wait until the router answers SSH (default 180s)

  console start|stop|status         run a background reader that logs the serial console (read-only)
  console log [lines]               print the tail of the console log (default 100)
  console wait <regex> [timeout]    wait for new console output matching a regex (default 180s)

  snapshot save [label]             copy a sysupgrade config backup of the router to local state
  snapshot restore <label>          restore a saved backup onto the router and reboot it
  snapshot list

  lan-client vlans                  list the VLANs on br-lan and the interface each one serves
  lan-client up <name> [--vid N] [--mac M] [--hostname H] [--dhcp-opts "O1 O2"]
                                    attach a synthetic LAN client to br-lan and lease an address
  lan-client exec <name> <cmd...>   run a command inside a synthetic client
  lan-client down <name>|--all
  lan-client list

Environment: WRT_HOST, OS_HOST, WRT_CONSOLE, WRT_CONSOLE_BAUD, MGMT_PORT, BENCH_STATE_DIR, PROFILE
EOF
}

die() {
	echo "bench: $*" >&2
	exit 1
}

wrt() { ssh -o BatchMode=yes -o ConnectTimeout=5 "$WRT_HOST" "$@"; }
wrt22() { ssh -p 22 -o BatchMode=yes -o ConnectTimeout=5 "$WRT_HOST" "$@"; }
os() { ssh -o BatchMode=yes -o ConnectTimeout=10 "$OS_HOST" "$@"; }

state_dir() {
	mkdir -p "$SNAPSHOT_DIR"
	chmod 700 "$BENCH_STATE_DIR" "$SNAPSHOT_DIR"
}

ok() { printf '  ok    %s\n' "$*"; }
bad() {
	printf '  FAIL  %s\n' "$*"
	PREFLIGHT_FAILED=1
}
note() { printf '  --    %s\n' "$*"; }

ssh_option() { ssh -G "$1" 2>/dev/null | awk -v k="$2" '$1 == k { print $2 }'; }

ssh_alias_configured() { [ "$(ssh_option "$1" hostname)" != "$1" ]; }

cmd_preflight() {
	local need_os=0
	case ${1:-} in
	--with-os) need_os=1 ;;
	"") ;;
	*) die "unknown preflight option: $1" ;;
	esac
	PREFLIGHT_FAILED=0
	echo "local"
	for tool in ssh tmux stty awk; do
		command -v "$tool" >/dev/null && ok "$tool" || bad "$tool not installed"
	done
	for host in "$WRT_HOST" "$OS_HOST"; do
		ssh_alias_configured "$host" && ok "ssh alias $host" || bad "ssh alias $host has no HostName in ~/.ssh/config"
	done

	if [ "$(ssh_option "$WRT_HOST" port)" = "$MGMT_PORT" ]; then
		ok "$WRT_HOST on management port $MGMT_PORT"
	else
		bad "$WRT_HOST is not on management port $MGMT_PORT (bench.sh mgmt install)"
	fi

	echo "router ($WRT_HOST)"
	if wrt true 2>/dev/null; then
		ok "ssh"
		wrt 'sh -s' <<'EOF' | sed 's/^/  /'
. /etc/openwrt_release 2>/dev/null
printf 'ok    board %s, %s\n' "$(cat /tmp/sysinfo/board_name 2>/dev/null)" "$DISTRIB_DESCRIPTION"
printf 'ok    startwrt sha256 %s\n' "$(sha256sum /usr/bin/startwrt | cut -c1-16)"
printf 'ok    uptime %s\n' "$(uptime | sed 's/^ *//')"
if ip link add bench-probe0 type veth peer name bench-probe1 2>/dev/null; then
	ip link del bench-probe0
	echo 'ok    veth (lan-client available)'
else
	echo 'FAIL  no veth support (kmod-veth): lan-client unavailable'
fi
for t in udhcpc nslookup nc curl tcpdump; do
	if command -v $t >/dev/null; then echo "ok    $t"; else echo "--    $t not installed"; fi
done
EOF
	else
		bad "ssh $WRT_HOST unreachable (is Remote Access on? recover over the console)"
	fi

	echo "LAN client ($OS_HOST)"
	local os_bad=bad
	[ "$need_os" = 1 ] || os_bad=note
	if os true 2>/dev/null; then
		ok "ssh (via $(ssh_option "$OS_HOST" proxyjump))"
		note "default route: $(os 'ip route show default' | head -1)"
		if os 'sudo -n start-cli git-info' >/dev/null 2>&1; then
			ok "sudo start-cli"
		else
			$os_bad "sudo start-cli failed on $OS_HOST"
		fi
	else
		$os_bad "ssh $OS_HOST unreachable"
	fi

	echo "console ($WRT_CONSOLE)"
	if [ -r "$WRT_CONSOLE" ]; then
		ok "device readable"
	elif [ -e "$WRT_CONSOLE" ]; then
		bad "no read access to $WRT_CONSOLE (see the udev rule in AGENTS.md)"
	else
		bad "$WRT_CONSOLE missing (adapter unplugged, or udev rule not installed)"
	fi
	console_running && ok "reader running, log $CONSOLE_LOG" || note "reader stopped (bench.sh console start)"

	[ "$PREFLIGHT_FAILED" = 0 ] || exit 1
}

cmd_deployed() {
	local bin=$REPO_ROOT/target/riscv64gc-unknown-linux-musl/${PROFILE:-release}/startwrt
	[ -f "$bin" ] || die "no local build at $bin"
	local here there
	here=$(sha256sum "$bin" | cut -d' ' -f1)
	there=$(wrt 'sha256sum /usr/bin/startwrt' | cut -d' ' -f1)
	if [ "$here" = "$there" ]; then
		echo "deployed: router runs $bin ($(cut -c1-16 <<<"$here"))"
	else
		echo "NOT deployed: local $(cut -c1-16 <<<"$here"), router $(cut -c1-16 <<<"$there")"
		exit 1
	fi
}

cmd_wait_ssh() {
	local timeout=${1:-180} start=$SECONDS
	until wrt true 2>/dev/null; do
		((SECONDS - start < timeout)) || die "router SSH not back after ${timeout}s"
		sleep 3
	done
	echo "router SSH up after $((SECONDS - start))s"
}

console_running() { tmux has-session -t "$CONSOLE_SESSION" 2>/dev/null; }

console_size() { stat -c %s "$CONSOLE_LOG" 2>/dev/null || echo 0; }

console_since() { tail -c +"$(($1 + 1))" "$CONSOLE_LOG" | tr -d '\r'; }

cmd_console() {
	local sub=${1:-status}
	shift || true
	case $sub in
	start)
		console_running && {
			echo "console reader already running"
			return
		}
		[ -r "$WRT_CONSOLE" ] || die "no read access to $WRT_CONSOLE"
		state_dir
		tmux new-session -d -s "$CONSOLE_SESSION" \
			"stty -F '$WRT_CONSOLE' $WRT_CONSOLE_BAUD raw -echo -crtscts -ixon -hupcl && exec cat '$WRT_CONSOLE' >>'$CONSOLE_LOG'"
		sleep 0.5
		console_running || die "console reader exited at once; check $WRT_CONSOLE"
		echo "console reader started, log $CONSOLE_LOG"
		;;
	stop)
		tmux kill-session -t "$CONSOLE_SESSION" 2>/dev/null && echo "console reader stopped" || echo "console reader not running"
		;;
	status)
		console_running && echo "running, log $CONSOLE_LOG" || echo "stopped"
		;;
	log)
		tail -n "${1:-100}" "$CONSOLE_LOG" | tr -d '\r'
		;;
	wait)
		local regex=${1:?regex required} timeout=${2:-180} from out start=$SECONDS
		console_running || die "console reader not running"
		from=$(console_size)
		until out=$(console_since "$from") && grep -Eq -- "$regex" <<<"$out"; do
			((SECONDS - start < timeout)) || die "no console output matching /$regex/ within ${timeout}s"
			sleep 1
		done
		echo "$out"
		;;
	*) die "unknown console command: $sub" ;;
	esac
}

cmd_snapshot() {
	local sub=${1:-list}
	shift || true
	state_dir
	case $sub in
	save)
		local label=${1:-$(date +%Y%m%d-%H%M%S)} file
		file=$SNAPSHOT_DIR/$label.tar.gz
		[ -e "$file" ] && die "snapshot $label exists"
		wrt 'umask 077; sysupgrade -b /tmp/bench-snapshot.tar.gz >/dev/null 2>&1'
		(
			umask 077
			wrt 'cat /tmp/bench-snapshot.tar.gz; rm -f /tmp/bench-snapshot.tar.gz' >"$file"
		)
		tar -tzf "$file" >/dev/null || {
			rm -f "$file"
			die "snapshot is not a valid archive"
		}
		echo "saved $label ($(tar -tzf "$file" | wc -l) files)"
		;;
	restore)
		local label=${1:?label required} file
		file=$SNAPSHOT_DIR/$label.tar.gz
		[ -f "$file" ] || die "no snapshot $label"
		wrt 'umask 077; cat >/tmp/bench-restore.tar.gz' <"$file"
		wrt 'sysupgrade -r /tmp/bench-restore.tar.gz && rm -f /tmp/bench-restore.tar.gz && { (sleep 1; reboot) </dev/null >/dev/null 2>&1 & }'
		echo "restored $label; router rebooting"
		sleep 10
		cmd_wait_ssh 240
		;;
	list)
		ls -1t "$SNAPSHOT_DIR" 2>/dev/null | sed 's/\.tar\.gz$//'
		;;
	*) die "unknown snapshot command: $sub" ;;
	esac
}

# Runs on the router (BusyBox ash) with the subcommand and its arguments.
LAN_CLIENT_REMOTE=$(
	cat <<'EOF'
set -e
BR=br-lan
DIR=/tmp/bench
filtering() { [ "$(cat /sys/class/net/$BR/bridge/vlan_filtering 2>/dev/null)" = 1 ]; }
mac_for() { echo "$1" | md5sum | sed 's/^\(..\)\(..\)\(..\)\(..\).*/02:b0:\1:\2:\3:\4/'; }
dhcp_script() {
	mkdir -p "$DIR"
	cat >"$DIR/udhcpc.sh" <<'SCRIPT'
#!/bin/sh
case "$1" in
deconfig) ip -4 addr flush dev "$interface" ;;
bound|renew)
	ip -4 addr flush dev "$interface"
	ip addr add "$ip/${mask:-24}" dev "$interface"
	[ -n "$router" ] && ip route replace default via "${router%% *}" dev "$interface"
	: >"$RESOLV"
	for d in $dns; do echo "nameserver $d" >>"$RESOLV"; done
	;;
esac
SCRIPT
	chmod +x "$DIR/udhcpc.sh"
}
sub=$1; shift
case $sub in
vlans)
	if filtering; then
		echo "vlan_filtering on; VIDs by interface:"
		for dev in /sys/class/net/$BR.*; do
			[ -e "$dev" ] || continue
			vid=${dev##*.}
			iface=$(uci show network | grep "\.device='$BR.$vid'" | head -1 | cut -d. -f2)
			printf '  vid %-5s %s %s\n' "$vid" "${iface:-?}" "$(ip -4 -br addr show dev $BR.$vid | awk '{print $3}')"
		done
	else
		echo "vlan_filtering off: one untagged LAN ($(ip -4 -br addr show dev $BR | awk '{print $3}'))"
	fi
	;;
up)
	name=$1; shift
	vid=; mac=; host=$name; opts=
	while [ $# -gt 0 ]; do
		case $1 in
		--vid) vid=$2; shift 2 ;;
		--mac) mac=$2; shift 2 ;;
		--hostname) host=$2; shift 2 ;;
		--dhcp-opts) opts=$2; shift 2 ;;
		*) echo "unknown option $1" >&2; exit 2 ;;
		esac
	done
	ns=bench-$name; veth=bv-$name
	[ ${#veth} -le 15 ] || { echo "name too long (max 12 chars)" >&2; exit 2; }
	[ -n "$mac" ] || mac=$(mac_for "$name")
	if ! ip netns list | grep -qw "$ns"; then
		ip netns add "$ns"
		ip link add "$veth" type veth peer name bp-$name
		ip link set bp-$name netns "$ns"
		ip -n "$ns" link set bp-$name name eth0
		ip -n "$ns" link set eth0 address "$mac"
		ip -n "$ns" link set lo up
	fi
	ip link set "$veth" master $BR up
	if filtering; then
		[ -n "$vid" ] || { echo "vlan_filtering is on: pass --vid (see lan-client vlans)" >&2; exit 2; }
		bridge vlan del dev "$veth" vid 1 2>/dev/null || true
		bridge vlan add dev "$veth" vid "$vid" pvid untagged
	elif [ -n "$vid" ] && [ "$vid" != 1 ]; then
		echo "vlan_filtering is off: --vid $vid has no effect" >&2; exit 2
	fi
	ip -n "$ns" link set eth0 up
	dhcp_script
	mkdir -p /etc/netns/$ns
	optargs=
	for o in $opts; do optargs="$optargs -O $o"; done
	ip netns exec "$ns" env RESOLV=/etc/netns/$ns/resolv.conf \
		udhcpc -i eth0 -n -q -t 5 -T 2 -s "$DIR/udhcpc.sh" -x hostname:"$host" $optargs >/dev/null
	echo "$name: mac $mac vid ${vid:-untagged}"
	ip -n "$ns" -br addr show dev eth0
	ip -n "$ns" route show default
	;;
exec)
	name=$1; shift
	ip netns exec "bench-$name" "$@"
	;;
down)
	if [ "$1" = --all ]; then
		names=$(ip netns list | awk '{print $1}' | sed -n 's/^bench-//p')
	else
		names=$1
	fi
	for n in $names; do
		ip link del "bv-$n" 2>/dev/null || true
		ip netns del "bench-$n" 2>/dev/null || true
		rm -rf "/etc/netns/bench-$n"
		echo "removed $n"
	done
	;;
list)
	for n in $(ip netns list | awk '{print $1}' | sed -n 's/^bench-//p'); do
		printf '%-12s %s %s\n' "$n" \
			"$(ip -n bench-$n -br link show dev eth0 | awk '{print $3}')" \
			"$(ip -n bench-$n -4 -br addr show dev eth0 | awk '{print $3}')"
	done
	;;
*) echo "unknown lan-client command: $sub" >&2; exit 2 ;;
esac
EOF
)

sq() { printf "'%s'" "${1//\'/\'\\\'\'}"; }

cmd_lan_client() {
	[ $# -ge 1 ] || die "lan-client needs a command"
	local args="" a
	for a in "$@"; do args+=" $(sq "$a")"; done
	wrt 'mkdir -p /tmp/bench && cat >/tmp/bench/lan-client.sh' <<<"$LAN_CLIENT_REMOTE"
	wrt "sh /tmp/bench/lan-client.sh$args"
}

is_rfc1918() {
	case $1 in
	10.* | 192.168.* | 172.1[6-9].* | 172.2[0-9].* | 172.3[01].*) return 0 ;;
	*) return 1 ;;
	esac
}

cmd_mgmt() {
	case ${1:-status} in
	install)
		local me
		me=$(wrt22 'echo "${SSH_CLIENT%% *}"') || die "install needs SSH on port 22, which Remote Access opens"
		wrt22 "set -e
uci -q delete dropbear.bench || true
uci set dropbear.bench=dropbear
uci set dropbear.bench.enable=1
uci set dropbear.bench.Port=$MGMT_PORT
uci set dropbear.bench.PasswordAuth=off
uci set dropbear.bench.RootPasswordAuth=off
uci commit dropbear
uci -q delete firewall.bench_mgmt || true
uci set firewall.bench_mgmt=rule
uci set firewall.bench_mgmt.name=bench_mgmt_ssh
uci set firewall.bench_mgmt.src=wan
uci set firewall.bench_mgmt.family=ipv4
uci set firewall.bench_mgmt.src_ip=$me
uci set firewall.bench_mgmt.proto=tcp
uci set firewall.bench_mgmt.dest_port=$MGMT_PORT
uci set firewall.bench_mgmt.target=ACCEPT
uci commit firewall
/etc/init.d/dropbear reload
/etc/init.d/firewall reload >/dev/null 2>&1"
		sleep 2
		ssh -p "$MGMT_PORT" -o BatchMode=yes -o ConnectTimeout=5 "$WRT_HOST" true ||
			die "management port $MGMT_PORT does not answer"
		echo "management SSH on port $MGMT_PORT, admitted from $me only"
		[ "$(ssh_option "$WRT_HOST" port)" = "$MGMT_PORT" ] ||
			echo "set 'Port $MGMT_PORT' under 'Host $WRT_HOST' in ~/.ssh/config"
		;;
	status)
		wrt "uci -q show dropbear.bench; uci -q show firewall.bench_mgmt" || die "no management SSH"
		;;
	remove)
		wrt22 "uci -q delete dropbear.bench; uci -q delete firewall.bench_mgmt; uci commit dropbear; uci commit firewall; /etc/init.d/firewall reload >/dev/null 2>&1; /etc/init.d/dropbear reload" ||
			die "remove needs SSH on port 22, which Remote Access opens"
		echo "management SSH removed; point $WRT_HOST back at port 22"
		;;
	*) die "unknown mgmt command: $1" ;;
	esac
}

cmd_smoke() {
	PREFLIGHT_FAILED=0
	local name=smoke gw wan
	echo "LAN (synthetic client)"
	cmd_lan_client down "$name" >/dev/null
	if cmd_lan_client up "$name" >/dev/null 2>&1; then
		ok "DHCP lease $(cmd_lan_client exec "$name" ip -4 -br addr show dev eth0 | awk '{print $3}')"
		gw=$(cmd_lan_client exec "$name" ip route show default | awk '{print $3; exit}')
		cmd_lan_client exec "$name" nslookup start9.com >/dev/null 2>&1 && ok "DNS" || bad "DNS lookup failed"
		[ "$(cmd_lan_client exec "$name" curl -s -o /dev/null -m 10 -w '%{http_code}' https://start9.com)" = 200 ] &&
			ok "HTTPS to the internet" || bad "HTTPS to the internet failed"
		[ "$(cmd_lan_client exec "$name" curl -sk -o /dev/null -m 10 -w '%{http_code}' "https://$gw/")" = 200 ] &&
			ok "router UI from LAN ($gw)" || bad "router UI from LAN ($gw) failed"
	else
		bad "synthetic client got no lease"
	fi
	cmd_lan_client down "$name" >/dev/null

	echo "WAN (this machine)"
	local mode expect ui ssh22
	wan=$(ssh_option "$WRT_HOST" hostname)
	mode=$(wrt 'uci -q get startwrt.preferences.remote_access' || echo default)
	case $mode in
	always) expect=open ;;
	never) expect=closed ;;
	*) is_rfc1918 "$wan" && expect=open || expect=closed ;;
	esac
	ui=$(curl -sk -o /dev/null -m 10 -w '%{http_code}' "https://$wan/" || true)
	wrt22 true 2>/dev/null && ssh22=open || ssh22=closed
	[ "$ui" = 200 ] && ui=open || ui=closed
	if [ "$ui" = "$expect" ] && [ "$ssh22" = "$expect" ]; then
		ok "Remote Access $mode: UI and SSH on port 22 $expect from WAN"
	else
		bad "Remote Access $mode: expected $expect from WAN, UI $ui, SSH on port 22 $ssh22"
	fi

	echo "router log"
	local errors
	errors=$(wrt "logread -e startwrt | grep -E ' ERROR |panicked'" || true)
	if [ -z "$errors" ]; then
		ok "no startwrt errors since boot"
	else
		bad "startwrt errors since boot:"
		sed 's/^/          /' <<<"$errors"
	fi

	echo "StartOS client ($OS_HOST)"
	if os true 2>/dev/null; then
		[ "$(os "curl -s -o /dev/null -m 10 -w '%{http_code}' https://start9.com")" = 200 ] &&
			ok "HTTPS to the internet" || bad "HTTPS to the internet failed"
	else
		note "unreachable; skipped"
	fi

	[ "$PREFLIGHT_FAILED" = 0 ] || exit 1
}

main() {
	local cmd=${1:-}
	shift || true
	case $cmd in
	preflight) cmd_preflight "$@" ;;
	deployed) cmd_deployed ;;
	mgmt) cmd_mgmt "$@" ;;
	smoke) cmd_smoke ;;
	wait-ssh) cmd_wait_ssh "$@" ;;
	console) cmd_console "$@" ;;
	snapshot) cmd_snapshot "$@" ;;
	lan-client) cmd_lan_client "$@" ;;
	-h | --help | help | "") usage ;;
	*)
		usage >&2
		exit 2
		;;
	esac
}

main "$@"
