import { writeFileSync } from 'node:fs'
import {
  Api,
  toolCatalog,
} from '../../../ts-modules/start-core/lib/osBindings/rpc/start-os.schemas'

writeFileSync(process.argv[2], `${JSON.stringify(toolCatalog(Api))}\n`)
