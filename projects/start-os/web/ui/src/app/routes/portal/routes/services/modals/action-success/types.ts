import { ActionRes } from 'src/app/services/api/api.types'

export type ActionResponse = NonNullable<ActionRes>
type ActionResult = NonNullable<ActionResponse['result']>
export type SingleResult = ActionResult & { type: 'single' }
export type GroupResult = ActionResult & { type: 'group' }
