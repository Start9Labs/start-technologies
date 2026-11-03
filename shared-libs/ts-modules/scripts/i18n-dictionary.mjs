import ts from 'typescript'

export function dictionaryEntries(source) {
  const entries = new Map()
  const file = ts.createSourceFile(
    'dictionary.ts',
    source,
    ts.ScriptTarget.Latest,
  )
  function visit(node) {
    if (
      ts.isPropertyAssignment(node) &&
      (ts.isStringLiteral(node.name) || ts.isNumericLiteral(node.name)) &&
      (ts.isStringLiteral(node.initializer) ||
        ts.isNumericLiteral(node.initializer))
    ) {
      entries.set(
        node.name.text,
        ts.isNumericLiteral(node.initializer)
          ? Number(node.initializer.text)
          : node.initializer.text,
      )
    }
    ts.forEachChild(node, visit)
  }
  visit(file)
  return entries
}
