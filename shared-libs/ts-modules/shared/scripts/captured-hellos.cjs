const { decodeFirst } = require('@start9labs/start-core/auth/cbor')
const {
  SESSION_PROFILE,
} = require('@start9labs/start-core/osBindings/session-profile')

function captureRecord(response, records) {
  const maximum = SESSION_PROFILE.transport.maxHandshakeRecordBytes + 2
  if (
    response.opcode === 2 &&
    response.payloadData.length <= Math.ceil(maximum / 3) * 4
  )
    records.push(response)
}

async function capturedHellos(responses) {
  const hellos = []
  for (const response of responses) {
    if (response.opcode !== 2) continue
    const wire = Buffer.from(response.payloadData, 'base64').subarray(2)
    try {
      const body = await decodeFirst(wire)
      if (body.value?.protocol !== SESSION_PROFILE.identity.protocol) continue
      const proof = await decodeFirst(wire.subarray(body.length))
      hellos.push({
        body: body.value,
        proof: proof.value,
        complete: body.length + proof.length === wire.length,
      })
    } catch {}
  }
  return hellos
}

module.exports = { captureRecord, capturedHellos }
