import { ErrorHandler, inject, Service } from '@angular/core'
import { TuiNotificationService } from '@taiga-ui/core'

@Service()
export class ErrorService extends ErrorHandler {
  private readonly alerts = inject(TuiNotificationService)

  override handleError(error: unknown, link?: string) {
    console.error(error)

    this.alerts
      .open(getErrorMessage(error, link), {
        label: 'Error',
        appearance: 'negative',
      })
      .subscribe()
  }
}

export function getErrorMessage(e: unknown, link?: string): string {
  const error = e !== null && typeof e === 'object' ? e : null
  let message = ''

  if (typeof e === 'string') {
    message = e
  } else if (error && 'code' in error && error.code === 0) {
    message =
      "Request Error. Your browser couldn't reach your server (network or browser blocked the connection). Common causes: a VPN or firewall blocking loopback/LAN; the server is still starting; or the page is loaded from a different origin than the API. Try refreshing this page; if you're using a VPN, allow LAN connections or disconnect briefly."
  } else if (
    error &&
    'message' in error &&
    typeof error.message === 'string' &&
    error.message
  ) {
    message = error.message
  } else {
    message = 'Unknown Error'
  }

  return link
    ? `${message}<br /><br /><a href=${link} target="_blank" rel="noreferrer">Get Help</a>`
    : message
}
