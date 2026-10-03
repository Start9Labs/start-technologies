const assert = require('node:assert/strict')
const { test, mock } = require('node:test')
const path = require('node:path')
const { randomUUID } = require('node:crypto')

async function runAction(inputs, files, uploadError) {
  const uploads = []
  const messages = []
  let outcome
  let finish
  const finished = new Promise(resolve => (finish = resolve))
  const core = {
    getInput: name => inputs[name] || '',
    info: message => {
      messages.push(message)
      if (message.startsWith('Uploaded ') || message.startsWith('No files')) {
        outcome = 'info'
        finish()
      }
    },
    warning: message => {
      messages.push(message)
      outcome = 'warning'
      finish()
    },
    setFailed: message => {
      messages.push(message)
      outcome = 'failed'
      finish()
    },
    setOutput: (name, value) => messages.push([name, value]),
  }
  mock.module('@actions/core', { namedExports: core })
  mock.module('@actions/glob', {
    namedExports: {
      create: async (pattern, options) => {
        assert.equal(pattern, inputs.pattern)
        assert.deepEqual(options, { matchDirectories: false })
        return { glob: async () => files }
      },
    },
  })
  mock.module('@actions/artifact', {
    namedExports: {
      DefaultArtifactClient: class {
        async uploadArtifact(...args) {
          uploads.push(args)
          if (uploadError) throw uploadError
          return { id: 42, size: 10 }
        }
      },
    },
  })
  try {
    await import(`../src/index.js?run=${randomUUID()}`)
    await finished
    return { uploads, messages, outcome }
  } finally {
    mock.restoreAll()
  }
}

test('uploads each file with its basename and preserves options', async () => {
  const files = [path.resolve('one.txt'), path.resolve('two.txt')]
  const { uploads, messages } = await runAction(
    { pattern: '*.txt', 'retention-days': '7', 'compression-level': '0' },
    [...files, `${path.resolve('directory')}${path.sep}`],
  )
  assert.deepEqual(uploads, [
    [
      'one.txt',
      [files[0]],
      process.cwd(),
      { compressionLevel: 0, retentionDays: 7 },
    ],
    [
      'two.txt',
      [files[1]],
      process.cwd(),
      { compressionLevel: 0, retentionDays: 7 },
    ],
  ])
  assert.ok(
    messages.some(
      message =>
        Array.isArray(message) &&
        message[0] === 'artifact-names' &&
        message[1] === '["one.txt","two.txt"]',
    ),
  )
})

test('uses repository retention and compression defaults', async () => {
  const file = path.resolve('one.txt')
  const { uploads } = await runAction({ pattern: '*.txt' }, [file])
  assert.deepEqual(uploads, [
    ['one.txt', [file], process.cwd(), { compressionLevel: 6 }],
  ])
})

for (const mode of ['warn', 'error', 'ignore']) {
  test(`handles no files in ${mode} mode without uploading`, async () => {
    const { uploads, messages, outcome } = await runAction(
      { pattern: '*.txt', 'if-no-files-found': mode },
      [],
    )
    assert.deepEqual(uploads, [])
    assert.deepEqual(messages, ['No files matched pattern: *.txt'])
    assert.equal(
      outcome,
      { warn: 'warning', error: 'failed', ignore: 'info' }[mode],
    )
  })
}

test('stops on upload failure and reports it', async () => {
  const { uploads, messages, outcome } = await runAction(
    { pattern: '*.txt' },
    [path.resolve('one.txt'), path.resolve('two.txt')],
    new Error('upload rejected'),
  )
  assert.equal(uploads.length, 1)
  assert.equal(outcome, 'failed')
  assert.match(messages.at(-1), /upload rejected/)
  assert.ok(!messages.some(Array.isArray))
})
