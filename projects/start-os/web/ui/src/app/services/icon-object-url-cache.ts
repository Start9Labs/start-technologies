import { catchError, defer, Observable, of, shareReplay } from 'rxjs'

export class IconObjectUrlCache {
  private readonly cache = new Map<string, Observable<string>>()

  constructor(
    private readonly load: (url: string) => Promise<string>,
    private readonly fallback: string,
  ) {}

  get(key: string, url: string): Observable<string> {
    const cached = this.cache.get(key)
    if (cached) return cached

    const icon$ = defer(() => this.load(url)).pipe(
      catchError(() => {
        this.cache.delete(key)
        return of(this.fallback)
      }),
      shareReplay(1),
    )
    this.cache.set(key, icon$)
    return icon$
  }
}
