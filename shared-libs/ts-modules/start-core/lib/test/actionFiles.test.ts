import { InputSpec } from '../actions/input/builder/inputSpec'
import { List } from '../actions/input/builder/list'
import { Value } from '../actions/input/builder/value'
import { Variants } from '../actions/input/builder/variants'
import { Action } from '../actions/setupActions'
import { Effects } from '../Effects'

const metadata = {
  name: 'Import',
  description: '',
  warning: null,
  allowedStatuses: 'any' as const,
  group: null,
  visibility: 'enabled' as const,
}

const file = {
  name: 'original.pem',
  path: 'keys/import.pem',
  mime: 'application/x-pem-file',
  commitment: { hash: 'authoritative-hash', size: 128 },
}

function effects() {
  return {
    eventId: 'file-event',
    getUploadLimits: jest.fn(async () => ({ maxSize: null })),
    resolveUpload: jest.fn(async () => file),
  } as unknown as Effects & {
    resolveUpload: jest.Mock
    getUploadLimits: jest.Mock
  }
}

test.each([0, 1024, null])(
  'File specs read maxSize %j from the host rather than duplicating it',
  async maxSize => {
    const host = effects()
    host.getUploadLimits.mockResolvedValue({ maxSize })
    const fields = [
      Value.file({
        uploadId: 'static',
        name: 'File',
        extensions: ['.pem'],
        required: true,
      }),
      Value.dynamicFile(async () => ({
        uploadId: 'dynamic',
        name: 'File',
        extensions: ['.pem'],
        required: true,
      })),
    ]
    for (const field of fields) {
      const built = await field.build({ effects: host, prefill: null })
      expect(built.spec).toMatchObject({
        type: 'file',
        maxSize,
        extensions: ['.pem'],
      })
    }
    expect(host.getUploadLimits.mock.calls).toEqual([
      [{ id: 'static' }],
      [{ id: 'dynamic' }],
    ])
  },
)

test.each([
  ['CERT.PEM', true],
  ['backup.TAR.GZ', true],
  ['backup.gz', false],
  ['wrong.txt', false],
])(
  'File input validates the uploaded filename %s against its form extensions',
  async (name, valid) => {
    const host = effects()
    const metadata = { ...file, name }
    host.resolveUpload.mockResolvedValue(metadata)
    const built = await Value.file({
      uploadId: 'upload',
      name: 'File',
      extensions: ['.pem', '.tar.gz'],
      required: true,
    }).build({ effects: host, prefill: null })
    if (valid)
      await expect(built.validator.parseAsync('upload')).resolves.toEqual(
        metadata,
      )
    else
      await expect(built.validator.parseAsync('upload')).rejects.toThrow(
        'Invalid file format',
      )
  },
)

test('browser-only file pickers keep File values without calling upload effects', async () => {
  const host = effects()
  const value = new File(['[Interface]'], 'gateway.conf')
  const field = Value.browserFile({
    name: 'Config',
    extensions: ['.conf'],
    required: true,
    maxSize: 30_000_000,
  })
  const built = await field.build({ effects: host, prefill: null })
  expect(built.spec).toMatchObject({
    type: 'file',
    uploadId: null,
    maxSize: 30_000_000,
  })
  expect(built.validator.parse(value)).toBe(value)
  expect(built.validator.safeParse(null).success).toBe(false)
  expect(host.getUploadLimits).not.toHaveBeenCalled()
  expect(host.resolveUpload).not.toHaveBeenCalled()
})

function requiredFile(uploadId = 'prepared-upload') {
  return Value.file({
    name: 'Certificate',
    uploadId,
    extensions: ['.pem'],
    required: true,
  })
}

test('File input resolves its prepared token into authoritative metadata before execution', async () => {
  const host = effects()
  const run = jest.fn(async () => null)
  const action = Action.withInput(
    'import',
    metadata,
    InputSpec.of({ file: requiredFile() }),
    async () => null,
    run,
  )
  const form = await action.getInput({ effects: host, prefill: null })
  expect(form.spec.file).toMatchObject({
    type: 'file',
    uploadId: 'prepared-upload',
    maxSize: null,
  })
  expect(host.getUploadLimits).toHaveBeenCalledWith({ id: 'prepared-upload' })
  expect(host.resolveUpload).not.toHaveBeenCalled()

  await action.run({ effects: host, input: { file: 'prepared-upload' } })

  expect(host.resolveUpload).toHaveBeenCalledWith({ id: 'prepared-upload' })
  expect(run).toHaveBeenCalledWith(expect.objectContaining({ input: { file } }))
})

test.each([
  null,
  undefined,
  'other-upload',
  { ...file },
  { id: 'prepared-upload' },
])('required File rejects forged or missing input %j', async input => {
  const host = effects()
  const run = jest.fn(async () => null)
  const action = Action.withInput(
    'import',
    metadata,
    InputSpec.of({ file: requiredFile() }),
    async () => null,
    run,
  )
  await action.getInput({ effects: host, prefill: null })
  await expect(
    action.run({ effects: host, input: { file: input } }),
  ).rejects.toThrow()
  expect(host.resolveUpload).not.toHaveBeenCalled()
  expect(run).not.toHaveBeenCalled()
})

test.each([null, undefined])(
  'nullable File maps %j to null without resolving',
  async input => {
    const host = effects()
    const built = await Value.file({
      name: 'Optional',
      uploadId: 'prepared-upload',
      extensions: [],
      required: false,
    }).build({ effects: host, prefill: null })
    await expect(built.validator.parseAsync(input)).resolves.toBeNull()
    expect(host.resolveUpload).not.toHaveBeenCalled()
  },
)

test('unready, failed, expired or wrong-event upload never reaches the action callback', async () => {
  const host = effects()
  host.resolveUpload.mockRejectedValue(new Error('Upload is not ready'))
  const run = jest.fn(async () => null)
  const action = Action.withInput(
    'import',
    metadata,
    InputSpec.of({ file: requiredFile() }),
    async () => null,
    run,
  )
  await action.getInput({ effects: host, prefill: null })
  await expect(
    action.run({ effects: host, input: { file: 'prepared-upload' } }),
  ).rejects.toThrow('Upload is not ready')
  expect(run).not.toHaveBeenCalled()
})

test('nested object, union and list File inputs use the same async validator', async () => {
  const host = effects()
  const spec = InputSpec.of({
    nested: Value.object(
      { name: 'Nested' },
      InputSpec.of({ file: requiredFile('object') }),
    ),
    choice: Value.union({
      name: 'Choice',
      default: 'upload',
      variants: Variants.of({
        upload: {
          name: 'Upload',
          spec: InputSpec.of({ file: requiredFile('union') }),
        },
        none: { name: 'None', spec: InputSpec.of({}) },
      }),
    }),
    files: Value.list(
      List.obj(
        { name: 'Files' },
        { spec: InputSpec.of({ file: requiredFile('list') }) },
      ),
    ),
  })
  const built = await spec.build({ effects: host, prefill: null })
  await expect(
    built.validator.parseAsync({
      nested: { file: 'object' },
      choice: { selection: 'upload', value: { file: 'union' } },
      files: [{ file: 'list' }],
    }),
  ).resolves.toEqual({
    nested: { file },
    choice: { selection: 'upload', value: { file } },
    files: [{ file }],
  })
  expect(host.resolveUpload.mock.calls.map(([{ id }]) => id).sort()).toEqual([
    'list',
    'object',
    'union',
  ])
})

test('dynamic File captures the id prepared while building this form', async () => {
  const host = effects()
  const built = await Value.dynamicFile(async () => ({
    name: 'Dynamic',
    uploadId: 'dynamic-upload',
    extensions: [],
    required: true,
  })).build({ effects: host, prefill: null })
  await expect(built.validator.parseAsync('dynamic-upload')).resolves.toEqual(
    file,
  )
  expect(host.resolveUpload).toHaveBeenCalledWith({ id: 'dynamic-upload' })
})

test('unrelated invalid fields can be corrected without consuming the upload receipt', async () => {
  const host = effects()
  const run = jest.fn(async () => null)
  const action = Action.withInput(
    'import',
    metadata,
    InputSpec.of({
      file: requiredFile(),
      count: Value.number({
        name: 'Count',
        default: null,
        required: true,
        integer: true,
        min: 1,
      }),
    }),
    async () => null,
    run,
  )
  await action.getInput({ effects: host, prefill: null })
  await expect(
    action.run({ effects: host, input: { file: 'prepared-upload', count: 0 } }),
  ).rejects.toThrow()
  expect(run).not.toHaveBeenCalled()
  await action.run({
    effects: host,
    input: { file: 'prepared-upload', count: 1 },
  })
  expect(run).toHaveBeenCalledWith(
    expect.objectContaining({ input: { file, count: 1 } }),
  )
})
