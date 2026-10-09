const assert = require('node:assert/strict')
const { createRequire } = require('node:module')
const path = require('node:path')
const test = require('node:test')

const root = path.resolve(__dirname, '../../..')
const ts = createRequire(path.join(root, 'projects/start-sdk/package.json'))(
  'typescript',
)

function editorDocumentation(entry) {
  const file = path.join(root, '__rpc_docs_probe__.ts')
  const text = `
import type { T } from '${entry}'
declare const r: T.ActionResult;
if (r.version === '1') { r.title; }
declare const v: T.ActionResultValue;
if (v.type === 'multiline') { v.value; v.masked; v.filename; }
declare const m: T.ActionResultMember;
if (m.type === 'multiline') { m.name; m.value; }
declare const metadata: T.ActionMetadataInput;
metadata.warning;
declare const alpn: T.AlpnInfoInput;
const response: T.ActionResultV1 = { title: 'Done', message: null, result: null };
`
  const options = {
    strict: true,
    skipLibCheck: true,
    target: ts.ScriptTarget.ES2021,
    module: ts.ModuleKind.CommonJS,
    moduleResolution: ts.ModuleResolutionKind.Node10,
    esModuleInterop: true,
    ignoreDeprecations: '6.0',
  }
  const host = {
    ...ts.sys,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
    getScriptFileNames: () => [file],
    getScriptVersion: () => '0',
    getScriptSnapshot: name => {
      const source = name === file ? text : ts.sys.readFile(name)
      return source === undefined
        ? undefined
        : ts.ScriptSnapshot.fromString(source)
    },
    getCurrentDirectory: () => root,
    getCompilationSettings: () => options,
    getDefaultLibFileName: settings => ts.getDefaultLibFilePath(settings),
  }
  const service = ts.createLanguageService(host)
  try {
    const diagnostics = [
      ...service.getSyntacticDiagnostics(file),
      ...service.getSemanticDiagnostics(file),
    ]
    assert.equal(
      diagnostics.length,
      0,
      ts.formatDiagnostics(diagnostics, {
        getCanonicalFileName: name => name,
        getCurrentDirectory: () => root,
        getNewLine: () => '\n',
      }),
    )
    for (const [probe, expected] of [
      ['r.title', 'Primary text to display as the header'],
      ['v.value', 'verbatim in a read-only monospace field'],
      ['v.masked', 'blur the value until the user reveals it'],
      ['v.filename', 'Also offer the value as a download'],
      ['m.name', 'A human-readable name or title'],
      ['m.value', 'verbatim in a read-only monospace field'],
      ['metadata.warning', 'Presents as an alert prior to executing'],
      ['T.AlpnInfoInput', 'The protocols a binding answers with'],
      ['title:', 'Primary text to display as the header'],
    ]) {
      const start = text.lastIndexOf(probe)
      assert.notEqual(start, -1, probe)
      const position =
        start + (probe.includes('.') ? probe.lastIndexOf('.') + 1 : 0)
      const info = service.getQuickInfoAtPosition(file, position)
      const documentation = ts
        .displayPartsToString(info?.documentation)
        .replace(/\s+/g, ' ')
      assert.ok(documentation.includes(expected), `${probe}: ${documentation}`)
    }
  } finally {
    service.dispose()
  }
}

for (const [name, entry] of [
  ['generated core types', './shared-libs/ts-modules/start-core/lib/index'],
  ['bundled SDK declarations', './projects/start-sdk/dist/lib/index'],
]) {
  test(`${name} preserve editor documentation`, () =>
    editorDocumentation(entry))
}
