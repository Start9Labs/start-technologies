function expressionBody(value) {
  return value.match(/^\$\{\{([\s\S]*?)\}\}$/)?.[1].trim()
}

function expression(value) {
  if (value === undefined || value === null) return "''"
  if (typeof value !== 'string') return JSON.stringify(value)
  return expressionBody(value) ?? `'${value.replaceAll("'", "''")}'`
}

function bindExpression(source, inputs, secrets, jobIds) {
  return source.replace(
    /'(?:[^']|'')*'|\b(inputs|secrets|needs)\.([\w-]+)/g,
    (reference, context, name) => {
      if (context === 'inputs') return `(${expression(inputs[name])})`
      if (context === 'secrets') return `(${expression(secrets[name])})`
      if (context === 'needs') return `needs.${jobIds[name]}`
      return reference
    },
  )
}

function bindValues(value, inputs, secrets, jobIds) {
  if (typeof value === 'string') {
    return value.replace(
      /\$\{\{([\s\S]*?)\}\}/g,
      (_, source) => `\${{${bindExpression(source, inputs, secrets, jobIds)}}}`,
    )
  }
  if (Array.isArray(value)) {
    return value.map(item => bindValues(item, inputs, secrets, jobIds))
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [
        key,
        bindValues(item, inputs, secrets, jobIds),
      ]),
    )
  }
  return value
}

function inlinePackageWorkflow(caller, readWorkflow) {
  const result = { ...caller, jobs: {} }

  function expand(call, prefix) {
    const workflow = readWorkflow(call.uses)
    const inputs = Object.fromEntries(
      Object.entries(workflow.on.workflow_call?.inputs || {}).map(
        ([name, specification]) => [
          name,
          call.with?.[name] ?? specification.default ?? '',
        ],
      ),
    )
    const secrets = call.secrets || {}
    const jobIds = Object.fromEntries(
      Object.keys(workflow.jobs).map(id => [id, `${prefix}_${id}`]),
    )
    for (const [id, source] of Object.entries(workflow.jobs)) {
      const job = bindValues(source, inputs, secrets, jobIds)
      const needs = [
        ...[call.needs || []].flat(),
        ...[source.needs || []].flat().map(id => jobIds[id]),
      ]
      if (needs.length) job.needs = [...new Set(needs)]
      const ownCondition =
        source.if === undefined
          ? undefined
          : bindExpression(
              expressionBody(source.if) ?? source.if,
              inputs,
              secrets,
              jobIds,
            )
      const conditions = [call.if, ownCondition].filter(
        value => value !== undefined,
      )
      if (conditions.length) {
        job.if = `\${{ ${conditions.map(value => `(${expressionBody(value) ?? value})`).join(' && ')} }}`
      }
      job.permissions ??= call.permissions
      if (job.permissions === undefined) delete job.permissions
      if (job.uses) expand(job, jobIds[id])
      else result.jobs[jobIds[id]] = job
    }
    return workflow
  }

  for (const [id, call] of Object.entries(caller.jobs)) {
    const workflow = expand(call, id)
    result.concurrency ??= workflow.concurrency
  }
  if (result.concurrency === undefined) delete result.concurrency
  return result
}

module.exports = { inlinePackageWorkflow }
