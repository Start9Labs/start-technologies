import assert from 'node:assert/strict'
import test from 'node:test'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(new Map())
const { prepareActionFiles, uploadActionFiles } =
  await import('../src/app/services/action-files.ts')

const fileSpec = uploadId => ({ type: 'file', uploadId })

test('prepares nested files without mutating form values or uploading inactive variants', () => {
  const first = new File(['first'], 'first.pem')
  const second = new File(['second'], 'second.txt')
  const third = new File(['third'], 'third.bin')
  const input = {
    text: 'unchanged',
    optional: null,
    nested: { file: first },
    union: { selection: 'chosen', value: { file: second } },
    list: [{ file: third }],
    textList: ['one', 'two'],
  }
  const prepared = prepareActionFiles(
    {
      text: { type: 'text' },
      optional: fileSpec('optional'),
      nested: { type: 'object', spec: { file: fileSpec('nested') } },
      union: {
        type: 'union',
        variants: {
          chosen: { spec: { file: fileSpec('union') } },
          unused: { spec: { file: fileSpec('unused') } },
        },
      },
      list: {
        type: 'list',
        spec: { type: 'object', spec: { file: fileSpec('list') } },
      },
      textList: { type: 'list', spec: { type: 'text' } },
    },
    input,
  )
  assert.deepEqual(prepared.input, {
    ...input,
    nested: { file: 'nested' },
    union: { selection: 'chosen', value: { file: 'union' } },
    list: [{ file: 'list' }],
  })
  assert.deepEqual(prepared.uploads, [
    { id: 'nested', file: first },
    { id: 'union', file: second },
    { id: 'list', file: third },
  ])
  assert.equal(input.nested.file, first)
  assert.equal(input.union.value.file, second)
  assert.equal(input.list[0].file, third)
})

test('browser-only file pickers are preserved without manufacturing an upload capability', () => {
  const file = new File(['[Interface]'], 'gateway.conf')
  assert.deepEqual(
    prepareActionFiles({ config: fileSpec(null) }, { config: file }),
    {
      input: { config: file },
      uploads: [],
    },
  )
})

test('missing optional files normalize to null', () => {
  assert.deepEqual(
    prepareActionFiles({ file: fileSpec('empty') }, { file: undefined }),
    {
      input: { file: null },
      uploads: [],
    },
  )
})

test('waits for each HTTP response, not merely for all bytes to be sent', async () => {
  const completed = []
  const progress = []
  const pending = []
  const task = uploadActionFiles(
    [
      { id: 'first', file: new File(['a'], 'a') },
      { id: 'second', file: new File([], 'empty') },
    ],
    async (id, file, signal, sent) => {
      assert.equal(signal.aborted, false)
      sent(file.size)
      await new Promise(resolve => pending.push(resolve))
    },
    new AbortController().signal,
    (id, loaded, complete) => {
      progress.push({ id, loaded, complete })
      if (complete) completed.push(id)
    },
  )
  await Promise.resolve()
  assert.deepEqual(completed, [])
  pending[0]()
  await Promise.resolve()
  await Promise.resolve()
  assert.deepEqual(completed, ['first'])
  pending[1]()
  await task
  assert.deepEqual(completed, ['first', 'second'])
  assert.deepEqual(progress, [
    { id: 'first', loaded: 1, complete: false },
    { id: 'second', loaded: 0, complete: false },
    { id: 'first', loaded: 1, complete: true },
    { id: 'second', loaded: 0, complete: true },
  ])
})

test('one failure aborts sibling uploads and waits for their cancellation before retry', async () => {
  const error = new Error('disk full')
  let fail
  let cancel
  let cancelled = false
  let settled = false
  const task = uploadActionFiles(
    [
      { id: 'first', file: new File(['a'], 'a') },
      { id: 'second', file: new File(['b'], 'b') },
    ],
    (id, file, signal) =>
      new Promise((resolve, reject) => {
        if (id === 'first') fail = () => reject(error)
        else
          signal.addEventListener(
            'abort',
            () => {
              cancelled = true
              cancel = () => reject(signal.reason)
            },
            { once: true },
          )
      }),
    new AbortController().signal,
    () => {},
  )
  task.catch(() => {
    settled = true
  })
  fail()
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(cancelled, true)
  assert.equal(settled, false)
  cancel()
  await assert.rejects(task, error)
})

test('the original failing upload is reported, not an earlier sibling cancellation', async () => {
  const failure = new Error('Server rejected the second file')
  const cancelled = new Error('Transport cancelled')
  await assert.rejects(
    uploadActionFiles(
      [
        { id: 'first', file: new File(['a'], 'a') },
        { id: 'second', file: new File(['b'], 'b') },
      ],
      (id, file, signal) =>
        id === 'second'
          ? Promise.reject(failure)
          : new Promise((resolve, reject) =>
              signal.addEventListener('abort', () => reject(cancelled), {
                once: true,
              }),
            ),
      new AbortController().signal,
      () => {},
    ),
    failure,
  )
})

test('closing the form aborts every in-flight upload', async () => {
  const abort = new AbortController()
  let cancelled = 0
  const task = uploadActionFiles(
    [
      { id: 'first', file: new File(['a'], 'a') },
      { id: 'second', file: new File(['b'], 'b') },
    ],
    (id, file, signal) =>
      new Promise((resolve, reject) => {
        signal.addEventListener(
          'abort',
          () => {
            cancelled++
            reject(signal.reason)
          },
          { once: true },
        )
      }),
    abort.signal,
    () => {},
  )
  abort.abort()
  await assert.rejects(task)
  assert.equal(cancelled, 2)
})

test('an already closed form starts no requests', async () => {
  const abort = new AbortController()
  abort.abort()
  let started = false
  await assert.rejects(
    uploadActionFiles(
      [{ id: 'file', file: new File(['a'], 'a') }],
      async () => {
        started = true
      },
      abort.signal,
      () => {},
    ),
  )
  assert.equal(started, false)
})
