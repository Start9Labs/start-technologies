import { FileHelper } from '../../util/fileHelper'
import { z } from '@start9labs/start-core/zExport'

const file = FileHelper.json(process.argv[2], z.object({ count: z.number() }))
async function run() {
  if (process.argv[3] === 'hold') {
    await file.update({} as any, async current => {
      process.stdout.write('locked\n')
      await new Promise(() => {})
      return current
    })
  } else if (process.argv[3] === 'write') {
    await file.write({} as any, { count: 1 })
  } else {
    for (let i = 0; i < Number(process.argv[3]); i++) {
      await file.update({} as any, async current => {
        await new Promise(resolve => setImmediate(resolve))
        return { count: (current?.count ?? 0) + 1 }
      })
    }
  }
}
run().catch(error => {
  console.error(error)
  process.exitCode = 1
})
