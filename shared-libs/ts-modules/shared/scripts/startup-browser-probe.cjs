const assert = require('node:assert/strict')
const { spawn } = require('node:child_process')
const { once } = require('node:events')
const fs = require('node:fs')
const http = require('node:http')
const net = require('node:net')
const path = require('node:path')
const readline = require('node:readline')
const { build } = require('esbuild')
const { captureRecord, capturedHellos } = require('./captured-hellos.cjs')
const {
  SIGNATURE_PROFILE,
} = require('@start9labs/start-core/osBindings/signature-profile')
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright')

;(async () => {
  const root = process.cwd()
  const shared = path.join(root, 'shared-libs/ts-modules/shared/src')
  await build({
    entryPoints: ['shared-libs/ts-modules/shared/scripts/startup-browser.ts'],
    outfile: 'target/startup-browser/client.js',
    bundle: true,
    platform: 'browser',
    plugins: [
      {
        name: 'production-startup-exports',
        setup(builder) {
          builder.onResolve({ filter: /^@start9labs\/shared$/ }, () => ({
            path: 'startup',
            namespace: 'startup',
          }))
          builder.onLoad({ filter: /.*/, namespace: 'startup' }, () => ({
            resolveDir: root,
            contents: `
        export { AuthKeyService } from ${JSON.stringify(path.join(shared, 'services/auth-key.service.ts'))};
        export { RpcSocketService } from ${JSON.stringify(path.join(shared, 'services/rpc-socket.service.ts'))};
        export { rpcRequestBody } from ${JSON.stringify(path.join(shared, 'services/rpc-request.ts'))};
        export { isRpcError } from ${JSON.stringify(path.join(shared, 'types/rpc.types.ts'))};
        export { RpcError } from ${JSON.stringify(path.join(shared, 'classes/rpc-error.ts'))};
      `,
          }))
        },
      },
    ],
  })
  const native = spawn(
    process.argv[2] || 'target/debug/examples/startup_browser',
    [],
    { stdio: ['ignore', 'pipe', 'inherit'] },
  )
  const lines = readline.createInterface({ input: native.stdout })
  let browser, server
  const connections = new Set()
  try {
    const [address] = await Promise.race([
      once(lines, 'line', { signal: AbortSignal.timeout(30000) }),
      once(native, 'exit').then(([code]) => {
        throw Error(`Native startup fixture exited: ${code}`)
      }),
    ])
    const endpoint = new URL(address)
    const bundle = fs.readFileSync('target/startup-browser/client.js')
    server = http.createServer((request, response) => {
      response.setHeader(
        'Content-Type',
        request.url === '/client.js' ? 'application/javascript' : 'text/html',
      )
      response.end(
        request.url === '/client.js'
          ? bundle
          : '<!doctype html><script src="/client.js"></script>',
      )
    })
    server.on('upgrade', (request, downstream, head) => {
      connections.add(downstream)
      downstream.on('close', () => connections.delete(downstream))
      const upstream = net.connect(endpoint.port, endpoint.hostname, () => {
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
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
    browser = await chromium.launch({
      headless: true,
      args: [
        '--no-sandbox',
        '--no-proxy-server',
        '--host-resolver-rules=MAP startup-http.local 127.0.0.1',
      ],
    })
    const page = await browser.newPage({ serviceWorkers: 'block' })
    const errors = []
    page.on('pageerror', error => errors.push(String(error)))
    const cdp = await page.context().newCDPSession(page)
    await cdp.send('Network.enable')
    let binary = 0,
      plaintext = 0
    const clientHellos = [],
      serverHellos = []
    cdp.on('Network.webSocketFrameSent', ({ response }) => {
      if (response.opcode === 1) plaintext++
      if (response.opcode === 2) binary++
      captureRecord(response, clientHellos)
    })
    cdp.on('Network.webSocketFrameReceived', ({ response }) =>
      captureRecord(response, serverHellos),
    )
    await page.goto(`http://startup-http.local:${server.address().port}/`)
    await page.waitForFunction(() => window.startupProbe)
    const result = await page.evaluate(() => startupProbe())
    assert.equal(result.secure, false)
    assert.equal(result.subtle, 'undefined')
    assert.equal(result.randomUUID, 'undefined')
    assert.deepEqual(result.states, Array(4).fill('initializing'))
    assert.equal(result.fresh, 'initializing')
    assert.equal(result.progress, true)
    assert.equal(result.initialBytes, 17 * 1024 * 1024)
    assert.equal(result.continuationBytes, 17 * 1024 * 1024)
    assert.equal(result.enrolled, null)
    assert.equal(result.canceled, true)
    assert.equal(result.freshIdentity, true)
    assert.deepEqual(result.records, [])
    const clientCaptures = await capturedHellos(clientHellos)
    const serverCaptures = await capturedHellos(serverHellos)
    assert.equal(plaintext, 0)
    assert.ok(binary > 8)
    assert.ok(clientCaptures.length >= 4)
    assert.equal(serverCaptures.length, clientCaptures.length)
    assert.equal(
      new Set(
        clientCaptures.map(hello => hello.proof.commitment.nonce.toString()),
      ).size,
      clientCaptures.length,
    )
    const prefix = SIGNATURE_PROFILE.ed25519.signaturePrefix
    for (const hello of [...clientCaptures, ...serverCaptures]) {
      assert.equal(hello.complete, true)
      assert.deepEqual(
        Array.from(hello.proof.signature.subarray(0, prefix.length)),
        prefix,
      )
    }
    assert.deepEqual(errors, [])
    const evidence = {
      ...result,
      binary,
      plaintext,
      hellos: clientCaptures.length,
      errors,
    }
    fs.writeFileSync(
      'target/startup-browser/results.json',
      JSON.stringify(evidence, null, 2),
    )
    console.log(JSON.stringify(evidence, null, 2))
  } finally {
    await browser?.close()
    for (const socket of connections) socket.destroy()
    if (server) await new Promise(resolve => server.close(resolve))
    lines.close()
    if (native.exitCode === null && native.signalCode === null) {
      const stopped = once(native, 'exit')
      native.kill('SIGTERM')
      await stopped
    }
  }
})().catch(error => {
  console.error(error)
  process.exitCode = 1
})
