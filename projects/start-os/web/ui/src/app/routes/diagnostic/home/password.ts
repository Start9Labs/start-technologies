import { inject, Service } from '@angular/core'
import {
  DialogService,
  ErrorService,
  i18nKey,
  TaskService,
} from '@start9labs/shared'
import { firstValueFrom } from 'rxjs'

const INCORRECT_PASSWORD = 7

/** Prompts for the server password only once the server asks for it. */
@Service()
export class DiagnosticPassword {
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)
  private readonly errors = inject(ErrorService)

  async run(
    loading: i18nKey,
    task: (password?: string) => Promise<unknown>,
  ): Promise<boolean> {
    let password: string | undefined
    while (true) {
      let rejected: unknown
      const done = await this.tasks.run(async () => {
        try {
          await task(password)
        } catch (e) {
          if ((e as { code?: unknown } | null)?.code !== INCORRECT_PASSWORD)
            throw e
          rejected = e
        }
      }, loading)
      if (!rejected) return done
      if (password !== undefined) this.errors.handleError(rejected)
      password = await firstValueFrom(
        this.dialog.openPrompt<string>({
          label: 'Password required',
          data: {
            message: 'Enter your master password to continue.',
            label: 'Master Password',
            placeholder: 'Enter master password',
            useMask: true,
          },
        }),
        { defaultValue: undefined },
      )
      if (password === undefined) return false
    }
  }
}
