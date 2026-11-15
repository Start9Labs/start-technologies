import { defer, finalize } from 'rxjs'
import type { Observable, ObservableInput } from 'rxjs'

/** Aborts the operation's signal when observation ends. */
export function withAbortSignal<T>(
  operation: (signal: AbortSignal) => ObservableInput<T>,
): Observable<T> {
  return defer(() => {
    const abort = new AbortController()
    return defer(() => operation(abort.signal)).pipe(
      finalize(() => abort.abort()),
    )
  })
}
