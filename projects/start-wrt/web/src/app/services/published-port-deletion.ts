import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { AffectedPublishedPort } from 'src/app/services/api/api.service'

/**
 * If `pending` is non-empty, prompt the user to confirm deleting the named
 * published ports (the pending change is about to break them). Returns true when
 * there is nothing to confirm or the user confirmed, false when they cancelled.
 */
export async function confirmPublishedPortDeletion(
  dialogs: TuiResponsiveDialogService,
  i18n: i18nPipe,
  pending: AffectedPublishedPort[],
): Promise<boolean> {
  if (!pending.length) return true
  return firstValueFrom(
    dialogs.open<boolean>(TUI_CONFIRM, {
      label: i18n.transform('Published Ports Will Be Deleted'),
      data: {
        content: fill(
          i18n.transform(
            'This change breaks the following published port(s), so they will be deleted: {list}.',
          ),
          { list: pending.map(p => p.label).join(', ') },
        ),
        yes: i18n.transform('Delete & Continue'),
        no: i18n.transform('Cancel'),
      },
    }),
  ).catch(() => false)
}
