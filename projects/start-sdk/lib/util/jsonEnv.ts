const JSON_ENV_CHUNK_BYTES = 64 * 1024

/** Encodes command variables in numbered UTF-8 JSON chunks. */
export function jsonEnv(
  environment: Record<string, string | null | undefined>,
): NodeJS.ProcessEnv {
  const result = { ...process.env }
  for (const name of Object.keys(result)) {
    if (/^JSON_ENV_[0-9]+$/.test(name)) delete result[name]
  }
  const json = Buffer.from(
    JSON.stringify(environment, (_key, value) =>
      value === undefined ? null : value,
    ),
  )
  let start = 0
  let index = 0
  while (start < json.length) {
    let end = Math.min(start + JSON_ENV_CHUNK_BYTES, json.length)
    while (end < json.length && (json[end] & 0xc0) === 0x80) end--
    result[`JSON_ENV_${index++}`] = json.toString('utf8', start, end)
    start = end
  }
  return result
}
