const assert = require('node:assert/strict')
const fs = require('node:fs')
const http = require('node:http')
const net = require('node:net')
const path = require('node:path')
const { once } = require('node:events')
const { captureRecord, capturedHellos } = require('./captured-hellos.cjs')
const {
  SIGNATURE_PROFILE,
} = require('@start9labs/start-core/osBindings/signature-profile')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright')
const native = new URL(process.argv[2])
const fingerprint = process.argv[3]
const bundle = fs.readFileSync(
  process.env.START_SESSION_BROWSER_BUNDLE ||
    path.resolve('target/signed-session-browser/client.js'),
)
;(async () => {
  let browser
  const connections = new Set()
  const server = http.createServer((request, response) => {
    if (request.url.startsWith('/probe/')) {
      const upstream = http.request(
        new URL(request.url, native),
        { method: request.method, headers: request.headers },
        result => {
          response.writeHead(result.statusCode, result.headers)
          result.pipe(response)
        },
      )
      upstream.on('error', error => response.destroy(error))
      request.pipe(upstream)
      return
    }
    response.setHeader('Cache-Control', 'no-store')
    response.setHeader(
      'Content-Type',
      request.url === '/client.js' ? 'application/javascript' : 'text/html',
    )
    response.end(
      request.url === '/client.js'
        ? bundle
        : '<script src="/client.js"></script>',
    )
  })
  server.on('upgrade', (request, downstream, head) => {
    connections.add(downstream)
    downstream.on('close', () => connections.delete(downstream))
    const upstream = net.connect(native.port, native.hostname, () => {
      upstream.write(
        `${request.method} ${request.url} HTTP/${request.httpVersion}\r\n` +
          request.rawHeaders.reduce(
            (text, value, index, all) =>
              index % 2 ? text : text + `${value}: ${all[index + 1]}\r\n`,
            '',
          ) +
          '\r\n',
      )
      if (head.length) upstream.write(head)
      downstream.pipe(upstream).pipe(downstream)
    })
    downstream.on('error', () => upstream.destroy())
    downstream.on('close', () => upstream.destroy())
    upstream.on('error', () => downstream.destroy())
  })
  try {
    server.listen(0, '127.0.0.1')
    await once(server, 'listening')
    browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
    const context = await browser.newContext({ serviceWorkers: 'block' })
    const page = await context.newPage()
    const errors = []
    page.on('pageerror', error => errors.push(String(error)))
    page.on('console', message => {
      if (message.type() === 'error') console.error(message.text())
    })
    const cdp = await context.newCDPSession(page)
    await cdp.send('Network.enable')
    let binary = 0,
      plaintext = 0
    const hellos = []
    const serverHellos = []
    const urls = []
    cdp.on('Network.webSocketCreated', ({ url }) => urls.push(new URL(url)))
    cdp.on('Network.webSocketFrameSent', ({ response }) => {
      if (response.opcode === 1) plaintext++
      if (response.opcode === 2) binary++
      captureRecord(response, hellos)
    })
    cdp.on('Network.webSocketFrameReceived', ({ response }) =>
      captureRecord(response, serverHellos),
    )
    await page.goto(`http://127.0.0.1:${server.address().port}/`)
    await page.waitForFunction(() => window.sessionProbe)
    assert.equal(await page.evaluate(() => sessionProbe.enroll()), false)
    assert.equal(
      await page.evaluate(() => sessionProbe.bulk()),
      17 * 1024 * 1024,
    )
    console.log(
      'PASS real Chromium opaque pure-Ed25519 client / pure-Ed25519 server and 17 MiB RPC reply',
    )
    await fetch(new URL('probe/size?bytes=0', native), { method: 'POST' })
    const ids = await page.evaluate(() => sessionProbe.sequence())
    assert.equal(new Set(ids).size, 8)
    console.log(
      'PASS eight awaited sequential RPC calls without manual disconnect',
    )
    const guid = await page.evaluate(() => sessionProbe.subscribe())
    await fetch(new URL(`probe/size?bytes=${5 * 1024 * 1024}`, native), {
      method: 'POST',
    })
    const revision = await page.evaluate(() => sessionProbe.revision())
    assert.deepEqual(revision, { length: 5 * 1024 * 1024 + 1, bom: 0xfeff })
    console.log(
      'PASS exact-GUID server-first admission and 5 MiB PatchDB revision with UTF-8 BOM',
    )
    await fetch(new URL('probe/size?bytes=0', native), { method: 'POST' })
    assert.equal(await page.evaluate(() => sessionProbe.numeric()), true)
    await page.reload()
    await page.waitForFunction(() => window.sessionProbe)
    await page.evaluate(() => sessionProbe.sequence())
    assert.deepEqual(await page.evaluate(() => sessionProbe.identity()), {
      extractable: false,
      fingerprint,
    })
    console.log(
      'PASS exact full-u64 reply ID, opaque IndexedDB reload and unchanged verified fingerprint',
    )
    assert.equal(await page.evaluate(() => sessionProbe.tamper()), true)
    assert.equal(await page.evaluate(() => sessionProbe.revoke()), true)
    const clientCaptures = await capturedHellos(hellos)
    const serverCaptures = await capturedHellos(serverHellos)
    assert.ok(
      clientCaptures.some(
        hello => hello.body.hello.target.continuation === guid,
      ),
    )
    assert.equal(plaintext, 0)
    assert.ok(binary > 20)
    assert.ok(clientCaptures.length >= 20)
    assert.equal(
      new Set(
        clientCaptures.map(hello => hello.proof.commitment.nonce.toString()),
      ).size,
      clientCaptures.length,
    )
    assert.ok(
      clientCaptures.every(
        hello => hello.body.protocol === 'Start-RPC-Session',
      ),
    )
    assert.ok(
      clientCaptures.every(
        hello =>
          hello.body.hello.ephemeral instanceof Uint8Array &&
          hello.proof.signer instanceof Uint8Array &&
          hello.proof.signature instanceof Uint8Array,
      ),
    )
    assert.ok(serverCaptures.length > 0)
    for (const hello of [...clientCaptures, ...serverCaptures]) {
      assert.equal(hello.complete, true)
      const prefix = SIGNATURE_PROFILE.ed25519.signaturePrefix
      assert.equal(
        hello.proof.signature.length,
        prefix.length + SIGNATURE_PROFILE.signatureBytes,
      )
      assert.ok(
        prefix.every((byte, index) => hello.proof.signature[index] === byte),
      )
    }
    assert.ok(
      urls.every(
        url =>
          !url.searchParams.has('secure') && !url.searchParams.has('context'),
      ),
    )
    assert.deepEqual(errors, [])
    console.log(
      'PASS AEAD tamper fails closed, logout invalidation, fresh per-socket nonces and binary-only RPC',
    )
    console.log(
      JSON.stringify({
        binary,
        plaintext,
        hellos: clientCaptures.length,
        errors,
      }),
    )
  } finally {
    await browser?.close()
    for (const socket of connections) socket.destroy()
    await new Promise(resolve => server.close(resolve))
  }
})().catch(error => {
  console.error(error)
  process.exitCode = 1
})
