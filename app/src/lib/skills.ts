import type { AgentInfo, NewTerm, Term } from './types';

export type SkillId = 'rtk' | 'caveman' | 'ponytail';

export const SKILLS: { id: SkillId; name: string }[] = [
  { id: 'rtk', name: 'RTK' },
  { id: 'caveman', name: 'Caveman' },
  { id: 'ponytail', name: 'Ponytail' },
];

const RTK_INSTALL = 'command -v rtk >/dev/null 2>&1 || curl -fsSL https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh | sh';

const RTK_FLAGS: Record<string, string> = {
  claude: '--auto-patch',
  codex: '--codex',
  gemini: '--gemini --auto-patch',
  opencode: '--opencode --auto-patch',
  copilot: '--copilot',
  cursor: '--agent cursor',
  agy: '--agent antigravity',
  vibe: '--agent vibe',
};

const SKILLS_CLI: Record<string, string> = {
  codex: 'codex',
  opencode: 'opencode',
  copilot: 'github-copilot',
  cursor: 'cursor',
  amp: 'amp',
  agy: 'antigravity',
  vibe: 'mistral-vibe',
};

const PLUGIN_REPO: Record<Exclude<SkillId, 'rtk'>, string> = {
  caveman: 'JuliusBrussee/caveman',
  ponytail: 'DietrichGebert/ponytail',
};

export function installScript(skill: SkillId, agent: string): string | null {
  if (skill === 'rtk') {
    const flags = RTK_FLAGS[agent];
    return flags ? `${RTK_INSTALL} && PATH="$HOME/.local/bin:$PATH" rtk init -g ${flags}` : null;
  }
  const repo = PLUGIN_REPO[skill];
  if (agent === 'claude') return `claude plugin marketplace add ${repo} && claude plugin install ${skill}@${skill}`;
  if (agent === 'gemini') return `gemini extensions install https://github.com/${repo}`;
  const slug = SKILLS_CLI[agent];
  return slug ? `npx -y skills add ${repo} --skill '*' -g -a ${slug} -y` : null;
}

export function installCmd(script: string, done: string): string[] {
  const quoted = done.replace(/'/g, `'\\''`);
  return ['/bin/sh', '-c', `${script}; s=$?; printf '\\n%s\\n' '${quoted}'; read _; exit $s`];
}

export function installTargets(skill: SkillId, agents: AgentInfo[]): AgentInfo[] {
  return agents.filter((a) => a.installed && installScript(skill, a.id) !== null);
}

export function startInstall(createTerm: (t: NewTerm) => Promise<Term>, skill: SkillId, agent: AgentInfo, done: string): Promise<Term> {
  const script = installScript(skill, agent.id);
  if (!script) return Promise.reject(new Error(`no install route for ${skill} on ${agent.name}`));
  const name = SKILLS.find((s) => s.id === skill)!.name;
  return createTerm({ cmd: installCmd(script, done), title: `${name} · ${agent.name}`, cols: 120, rows: 32 });
}
