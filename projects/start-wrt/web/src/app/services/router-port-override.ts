import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { RouterPortCollision } from 'src/app/services/api/api.service'

/**
 * If `pending` is non-empty, prompt the user to confirm publishing port(s) the
 * router itself answers on from the WAN (remote access to its web interface,
 * SSH, or a VPN server) — the forward would capture that traffic. Returns true
 * when there is nothing to confirm or the user confirmed, false when they
 * cancelled.
 */
export async function confirmRouterPortOverride(
  dialogs: TuiResponsiveDialogService,
  i18n: i18nPipe,
  pending: RouterPortCollision[],
): Promise<boolean> {
  if (!pending.length) return true
  const ports = [...new Set(pending.flatMap(c => c.router_ports))].join(', ')
  return firstValueFrom(
    dialogs.open<boolean>(TUI_CONFIRM, {
      label: i18n.transform('Port Used by This Router'),
      data: {
        content: fill(
          i18n.transform(
            'Port(s) {ports} are used by this router itself — for remote access to its web interface, SSH, or a VPN server. Publishing them will send that traffic to the selected device instead, cutting those services off from outside your network. Publish anyway?',
          ),
          { ports },
        ),
        yes: i18n.transform('Publish Anyway'),
        no: i18n.transform('Cancel'),
      },
    }),
  ).catch(() => false)
}
