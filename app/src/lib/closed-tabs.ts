export const MAX_CLOSED = 10;

export function pushClosed(stack: string[], key: string, max = MAX_CLOSED): string[] {
  return [key, ...stack.filter((k) => k !== key)].slice(0, max);
}

export function popReopenable(stack: string[], reopen: (key: string) => string | null): { key: string | null; rest: string[] } {
  for (let i = 0; i < stack.length; i++) {
    const key = reopen(stack[i]);
    if (key) return { key, rest: stack.slice(i + 1) };
  }
  return { key: null, rest: [] };
}

export const closedShells = new Map<string, string>();

export function rememberShell(key: string, cwd: string) {
  closedShells.delete(key);
  closedShells.set(key, cwd);
  if (closedShells.size > MAX_CLOSED * 5) closedShells.delete(closedShells.keys().next().value!);
}
