import './zod-config'

import { Buffer } from 'buffer'
;(window as any).global = window
;(window as any).process = { env: { DEBUG: undefined }, browser: true }
;(window as any).Buffer = Buffer
