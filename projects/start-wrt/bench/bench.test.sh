#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/bench.sh" --help >/dev/null
TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT
WRT_CONSOLE=$TEST_DIR/console
BENCH_PASSWORD_FILE=$TEST_DIR/password
printf 'fixture "password"\\value\n' >"$BENCH_PASSWORD_FILE"
chmod 600 "$BENCH_PASSWORD_FILE"
touch "$WRT_CONSOLE"

preflight_fixture() {
	tmux() { return 0; }
	console_running() { return 1; }
	ssh_option() {
		case $1:$2 in
		"$WRT_HOST":hostname) echo 192.0.2.1 ;;
		"$WRT_HOST":port) echo "$MGMT_PORT" ;;
		"$OS_HOST":hostname) echo "${OS_ALIAS:-$OS_HOST}" ;;
		esac
	}
	os() { return 1; }
	ui_rpc() { echo '{"result":{}}'; }
	ip() {
		if [ "${VETH_AVAILABLE:-1}" = 0 ]; then return 1; fi
	}
	sha256sum() { echo 'fixture  /usr/bin/startwrt'; }
	cat() {
		if [ "$1" = /tmp/sysinfo/board_name ]; then echo fixture; else command cat "$@"; fi
	}
	export -f ip sha256sum cat
	wrt() {
		if [ "$1" = true ]; then return 0; fi
		if [ "${PROBE_TRANSPORT_FAILURE:-0}" = 1 ]; then return 255; fi
		VETH_AVAILABLE=${VETH_AVAILABLE:-1} DISTRIB_DESCRIPTION=fixture bash -s
	}
}

optional_os_alias() {
	preflight_fixture
	cmd_preflight
}

required_os_alias() {
	preflight_fixture
	cmd_preflight --with-os
}

missing_veth() {
	preflight_fixture
	OS_ALIAS=fixture-os
	VETH_AVAILABLE=0
	cmd_preflight
}

failed_router_probe() {
	preflight_fixture
	OS_ALIAS=fixture-os
	PROBE_TRANSPORT_FAILURE=1
	cmd_preflight
}

smoke_vlan_client() {
	cmd_lan_client() {
		case $1 in
		down) ;;
		up) [ "$*" = 'up smoke --vid 1' ] ;;
		exec)
			case $3 in
			ip)
				if [ "$4" = route ]; then echo 'default via 192.0.2.1'; else echo 'eth0 UP 192.0.2.2/24'; fi
				;;
			nslookup) ;;
			curl) echo 200 ;;
			esac
			;;
		esac
	}
	ssh_option() { echo 192.0.2.1; }
	wrt() {
		case $1 in
		'uci -q get startwrt.preferences.remote_access') echo always ;;
		esac
	}
	wrt22() { return 0; }
	curl() { echo 200; }
	os() { return 1; }
	cmd_smoke
}

rpc_json_encoding() {
	python3() { return 127; }
	BENCH_STATE_DIR=$TEST_DIR/state
	SNAPSHOT_DIR=$BENCH_STATE_DIR/snapshots
	UI_KEY=$BENCH_STATE_DIR/ui-key.pem
	wrt() {
		if [ "$1" = 'uci -q get network.lan.ipaddr' ]; then echo 192.0.2.1/24; else command cat >"$TEST_DIR/requests"; fi
	}
	ui_rpc fixture.method '{"text":"quoted \\\"value\\\"","nested":[1,true]}'
	[ "$(stat -c %a "$UI_KEY")" = 600 ]
	node - "$TEST_DIR/requests" "$BENCH_PASSWORD_FILE" "$REPO_ROOT" <<'JS'
const crypto = require('node:crypto')
const fs = require('node:fs')
const assert = require('node:assert/strict')
const { blake3 } = require(require.resolve('@noble/hashes/blake3', { paths: [process.argv[4]] }))
const [ip, login, loginSig, req, reqSig, retrySig] = fs.readFileSync(process.argv[2], 'utf8').trim().split('\n')
assert.equal(ip, '192.0.2.1')
const loginBody = JSON.parse(login)
assert.equal(loginBody.method, 'auth.login')
assert.equal(loginBody.params.password, fs.readFileSync(process.argv[3], 'utf8').trimEnd())
const pubkey = crypto.createPublicKey(loginBody.params.pubkey)
assert.equal(JSON.parse(req).method, 'fixture.method')
assert.deepEqual(JSON.parse(req).params.nested, [1, true])
const nonces = new Set()
for (const [body, header] of [[login, loginSig], [req, reqSig], [req, retrySig]]) {
	const h = new URLSearchParams(header)
	const bytes = Buffer.from(body)
	assert.equal(h.get('commitment'), 'request')
	assert.equal(h.get('size'), String(bytes.length))
	assert.equal(h.get('blake3'), Buffer.from(blake3(bytes)).toString('base64url'))
	assert.deepEqual(Buffer.from(h.get('signer'), 'base64url'), pubkey.export({ type: 'spki', format: 'der' }))
	nonces.add(h.get('nonce'))
	const commitment = Buffer.alloc(24)
	commitment.writeBigInt64BE(BigInt(h.get('timestamp')), 0)
	commitment.writeBigUInt64BE(BigInt(h.get('nonce')), 8)
	commitment.writeBigUInt64BE(BigInt(bytes.length), 16)
	const message = Buffer.concat([Buffer.from('Start-Auth-Sig v2\0'), Buffer.from([ip.length]), Buffer.from(ip), Buffer.from([3]), commitment, blake3(bytes)])
	const signature = Buffer.from(h.get('signature'), 'base64url').subarray(11)
	assert.ok(crypto.verify(null, message, pubkey, signature))
}
assert.equal(nonces.size, 3)
JS
	ui_rpc fixture.default
	node - "$TEST_DIR/requests" <<'JS'
const fs = require('node:fs')
const assert = require('node:assert/strict')
const req = JSON.parse(fs.readFileSync(process.argv[2], 'utf8').trim().split('\n')[3])
assert.deepEqual(req.params, {})
JS
}

invalid_rpc_json() {
	wrt() { echo 'unexpected SSH call' >&2; return 99; }
	ui_rpc fixture.method '{invalid'
}

FAILURES=0
check() {
	local expected=$1 test=$2 status
	set +e
	(set -e; "$test") >"$TEST_DIR/output" 2>&1
	status=$?
	set -e
	if [ "$status" = "$expected" ]; then
		printf 'PASS %s\n' "$test"
	else
		printf 'FAIL %s: expected exit %s, got %s\n' "$test" "$expected" "$status"
		command cat "$TEST_DIR/output"
		FAILURES=$((FAILURES + 1))
	fi
}

check 0 optional_os_alias
check 1 required_os_alias
check 1 missing_veth
check 1 failed_router_probe
check 0 smoke_vlan_client
check 0 rpc_json_encoding
check 1 invalid_rpc_json
[ "$FAILURES" = 0 ]
