/** The server expects forward slashes; Windows users type backslashes. */
export function normalizeCifsPath(path: string): string {
  return path.replaceAll('\\', '/')
}
