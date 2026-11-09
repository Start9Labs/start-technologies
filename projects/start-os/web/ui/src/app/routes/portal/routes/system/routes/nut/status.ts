import { i18nKey } from '@start9labs/shared'

export type Appearance = 'positive' | 'warning' | 'negative'

export interface UpsPower {
  label: i18nKey
  icon: string
  appearance: Appearance
}

export interface UpsView {
  model: string | null
  power: UpsPower
  charging: boolean
  batteryAppearance: Appearance
  loadAppearance: Appearance
  alerts: i18nKey[]
  inputVoltage: number | null
  charge: number | null
  runtime: number | null
  shutdownCharge: number | null
  shutdownRuntime: number | null
  load: number | null
  watts: number | null
  nominalWatts: number | null
}

const ALERTS: Record<string, i18nKey> = {
  RB: 'Replace battery',
  OVER: 'Overloaded',
}

export function toUpsView(vars: Record<string, string>): UpsView {
  const flags = vars['ups.status']?.split(' ') || []
  const number = (key: string) => {
    const value = Number.parseFloat(vars[key] ?? '')
    return Number.isFinite(value) ? value : null
  }
  const load = number('ups.load')
  const charge = number('battery.charge')
  const lowCharge =
    charge !== null && charge <= (number('battery.charge.warning') ?? 20)
  const onBattery = flags.includes('OB')
  const critical = flags.includes('LB') || flags.includes('FSD')
  const nominalWatts = number('ups.realpower.nominal')

  return {
    model:
      [
        vars['device.mfr'] ?? vars['ups.mfr'],
        vars['device.model'] ?? vars['ups.model'],
      ]
        .filter(Boolean)
        .join(' ') || null,
    power: flags.includes('FSD')
      ? { label: 'Shutting down', icon: '@tui.power', appearance: 'negative' }
      : flags.includes('LB')
        ? {
            label: 'Low battery',
            icon: '@tui.battery-warning',
            appearance: 'negative',
          }
        : flags.includes('OB')
          ? {
              label: 'On battery',
              icon: '@tui.battery-medium',
              appearance: 'warning',
            }
          : {
              label: 'On wall power',
              icon: '@tui.plug-zap',
              appearance: 'positive',
            },
    charging: flags.includes('CHRG'),
    batteryAppearance:
      critical || (onBattery && lowCharge)
        ? 'negative'
        : onBattery || lowCharge
          ? 'warning'
          : 'positive',
    loadAppearance:
      flags.includes('OVER') || (load ?? 0) >= 90
        ? 'negative'
        : (load ?? 0) >= 70
          ? 'warning'
          : 'positive',
    alerts: flags.flatMap(flag => (ALERTS[flag] ? [ALERTS[flag]] : [])),
    inputVoltage: number('input.voltage'),
    charge,
    runtime: number('battery.runtime'),
    shutdownCharge: number('battery.charge.low'),
    shutdownRuntime: number('battery.runtime.low'),
    load,
    watts:
      number('ups.realpower') ??
      (load !== null && nominalWatts !== null
        ? Math.round((load * nominalWatts) / 100)
        : null),
    nominalWatts,
  }
}
