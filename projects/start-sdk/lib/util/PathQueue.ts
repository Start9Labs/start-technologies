export class PathQueue {
  private readonly tails = new Map<string, Promise<void>>()

  run<A>(path: string, operation: () => Promise<A>): Promise<A> {
    const result = (this.tails.get(path) ?? Promise.resolve()).then(operation)
    const tail = result.then(
      () => {},
      () => {},
    )
    this.tails.set(path, tail)
    void tail.then(() => {
      if (this.tails.get(path) === tail) this.tails.delete(path)
    })
    return result
  }
}
