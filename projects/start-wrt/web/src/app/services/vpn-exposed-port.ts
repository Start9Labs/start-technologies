import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'

/**
 * A published-port exposure to confirm: the device's security `profile` routes
 * through a VPN (`vpn` label), but the named published port `labels` are still
 * reached over the public WAN IP, not the VPN.
 */
export interface VpnExposure {
  profile: string
  vpn: string
  labels: string[]
}

/**
 * Warn that published ports stay reachable over the public WAN IP even though
 * the device's security profile routes through a VPN, and ask the user to
 * confirm before exposing them. Returns true when there is nothing to warn
 * about (`exposure` is null or has no ports) or the user chose to continue,
 * false when they cancelled.
 *
 * Shared by every action that newly exposes a port despite VPN routing —
 * creating/editing a port, re-enabling a disabled one, or switching a profile's
 * outbound to a VPN — so the warning is uniform and fires once, at the moment
 * exposure is actually created, rather than perpetually in a form.
 */
export async function confirmVpnExposedPort(
  dialogs: TuiResponsiveDialogService,
  i18n: i18nPipe,
  exposure: VpnExposure | null,
): Promise<boolean> {
  if (!exposure || !exposure.labels.length) return true
  const list =
    exposure.labels.slice(0, 3).join(', ') +
    (exposure.labels.length > 3 ? ` (+${exposure.labels.length - 3})` : '')
  return firstValueFrom(
    dialogs.open<boolean>(TUI_CONFIRM, {
      label: i18n.transform('Published Port Will Be Exposed'),
      data: {
        content: fill(
          i18n.transform(
            'The "{profile}" profile routes traffic through a VPN ({vpn}), but published ports are reached over your public internet (WAN) connection, not the VPN. The following port(s) will stay exposed on your real public IP address: {list}.',
          ),
          { profile: exposure.profile, vpn: exposure.vpn, list },
        ),
        yes: i18n.transform('Expose & Continue'),
        no: i18n.transform('Cancel'),
      },
    }),
  ).catch(() => false)
}
