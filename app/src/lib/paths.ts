export function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

export const trimSlashes = (p: string) => (p.length > 1 ? p.replace(/\/+$/, '') : p);

export function folderName(path: string): string {
  return trimSlashes(path.trim()).split('/').pop() ?? '';
}
