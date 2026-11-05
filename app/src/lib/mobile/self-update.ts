type Env = Record<string, string | undefined>;

export function selfUpdate(env: Env = (globalThis as { process?: { env: Env } }).process?.env ?? {}): boolean {
  return env.OVERSEER_DISTRIBUTION !== 'play' && env.TAURI_ENV_PLATFORM !== 'ios';
}
