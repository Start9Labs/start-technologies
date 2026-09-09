import { config } from 'zod/v4/core'

// The UI CSP forbids Zod's JIT compiler.
config({ jitless: true })
