import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { DeviceLeavingProfile } from 'src/app/services/api/api.service'

export async function confirmLeavingProfile(
  dialogs: TuiResponsiveDialogService,
  i18n: i18nPipe,
  leaving: DeviceLeavingProfile[],
): Promise<boolean> {
  if (!leaving.length) return true
  return firstValueFrom(
    dialogs.open<boolean>(TUI_CONFIRM, {
      label: i18n.transform('Devices Will Leave Their Profile'),
      data: {
        content: leavingProfileMessage(i18n, leaving),
        yes: i18n.transform('Continue'),
        no: i18n.transform('Cancel'),
      },
    }),
  ).catch(() => false)
}

/** One sentence per profile the devices leave; empty for none. */
export function leavingProfileMessage(
  i18n: i18nPipe,
  leaving: DeviceLeavingProfile[],
): string {
  const byProfile = new Map<string, string[]>()
  for (const { profile, name, mac } of leaving) {
    byProfile.set(profile, [...(byProfile.get(profile) ?? []), name || mac])
  }
  return [...byProfile]
    .map(([profile, names]) =>
      fill(
        i18n.transform(
          "{list} will keep their reserved addresses and published ports for the {profile} profile. These apply again when they're back on it.",
        ),
        { list: names.join(', '), profile },
      ),
    )
    .join(' ')
}
