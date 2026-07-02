import { InjectionToken } from '@angular/core'

export type WorkspaceConfig = {
  useMocks: boolean
  api: {
    url: string
    version: string
  }
  gitHash: string
}

export const IS_MOCK = new InjectionToken<boolean>('IS_MOCK')
export const GIT_HASH = new InjectionToken<string>('GIT_HASH')
