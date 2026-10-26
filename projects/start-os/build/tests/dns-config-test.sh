#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
node <<'NODE'
const assert = require('node:assert/strict')
const fs = require('node:fs')
const os = require('node:os')
const path = require('node:path')
const { spawnSync } = require('node:child_process')
const read = path => fs.readFileSync(path, 'utf8')
const base = 'projects/start-os/'
const config = read(base + 'build/lib/resolved.conf')
const values = Object.fromEntries(config.split('\n').filter(line => line.includes('=')).map(line => line.split('=')))
assert.equal(values.DNSStubListener, 'no')
assert.equal(values.MulticastDNS, 'no')
assert.equal(values.LLMNR, 'no')
assert.equal(values.Cache, 'no')
assert.equal(values.FallbackDNS, '')
assert.equal(values.DNS, undefined, 'resolved must read the OS stub from resolv.conf')
const postinst = read(base + 'debian/postinst')
const nm = postinst.match(/cat << EOF > \/etc\/NetworkManager\/NetworkManager.conf\n([\s\S]*?)\nEOF/)[1]
assert.match(nm, /^dns=none$/m)
for (const [script, server] of [['debian/postinst', '127.0.0.1'], ['container-runtime/deb-install.sh', '10.0.3.1']]) {
  const source = read(base + script)
  assert.deepEqual([...source.matchAll(/echo "nameserver ([^"]+)"/g)].map(match => match[1]), [server])
  assert.match(source, /^cp \/usr\/lib\/startos\/resolved.conf \/etc\/systemd\/resolved.conf$/m)
}
assert.match(read(base + 'container-runtime/update-image.sh'), /^cp \.\.\/build\/lib\/resolved.conf \/tmp\/combined\/usr\/lib\/startos\/resolved.conf$/m)
assert.match(read(base + 'build.mk'), /^projects\/start-os\/container-runtime\/rootfs\.\$\(ARCH\)\.squashfs: .*projects\/start-os\/build\/lib\/resolved.conf/m)
const resolverSetup = postinst.match(/ln -sf \/usr\/share\/zoneinfo\/Etc\/UTC \/etc\/localtime\n\n([\s\S]*?)\n\necho "#" > \/etc\/network\/interfaces/)[1]
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'startos-dns-'))
try {
  for (const marker of [null, 'upgrade', 'live']) {
    const root = path.join(tmp, marker ?? 'runtime')
    fs.mkdirSync(root + '/etc', { recursive: true })
    const buildResolver = 'search build.invalid\nnameserver 192.0.2.53\n'
    fs.writeFileSync(root + '/etc/upstream', buildResolver)
    if (marker === 'live') {
      fs.writeFileSync(root + '/etc/resolv.conf', buildResolver)
      fs.symlinkSync('upstream', root + '/etc/resolv.conf.orig')
    } else {
      fs.symlinkSync('upstream', root + '/etc/resolv.conf')
    }
    if (marker !== null) fs.writeFileSync(root + '/etc/debian_chroot', marker + '\n')
    const result = spawnSync('sh', ['-ec', resolverSetup.replaceAll('/etc/', root + '/etc/')], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.equal(read(root + '/etc/upstream'), buildResolver, 'must replace resolver symlinks without modifying their targets')
    if (marker === 'live') {
      assert.equal(read(root + '/etc/resolv.conf'), buildResolver, 'package installs and NVIDIA/Tor hooks retain build DNS')
      assert.equal(read(root + '/etc/resolv.conf.orig'), 'nameserver 127.0.0.1\n')
      fs.renameSync(root + '/etc/resolv.conf.orig', root + '/etc/resolv.conf')
    }
    assert.equal(fs.lstatSync(root + '/etc/resolv.conf').isSymbolicLink(), false)
    assert.equal(read(root + '/etc/resolv.conf'), 'nameserver 127.0.0.1\n', 'runtime must have only the OS stub, without build DNS or search domains')
  }
} finally {
  fs.rmSync(tmp, { recursive: true, force: true })
}
const recipe = read(base + 'build/image-recipe/build.sh')
assert.doesNotMatch(recipe, /resolv\.conf\.bak/, 'must not restore build-host DNS into the shipped rootfs')
assert.match(recipe, /lb chroot\nlb installer\nlb binary_chroot\nlb chroot_prep install all mode-apt-install-binary mode-archives-chroot\nlb binary_rootfs\n[\s\S]*?if \[ "\$\{IMAGE_TYPE\}" = iso \]; then/, 'ISO and img share live-build resolver cleanup and rootfs assembly')
console.log('DNS configuration preserves build DNS until live-build restores the runtime OS stub')
NODE
