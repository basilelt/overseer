import { describe, expect, it, vi } from 'vitest';
import { installCmd, installScript, installTargets, SKILLS, startInstall } from './skills';
import type { AgentInfo, NewTerm, Term } from './types';

const agent = (id: string, installed = true): AgentInfo => ({ id, name: id, installed, efforts: [], resume: false, verified: true, sessions: true, turns: true });

describe('installScript', () => {
  it('installs Caveman and Ponytail as Claude Code plugins from their marketplace', () => {
    expect(installScript('caveman', 'claude')).toBe('claude plugin marketplace add JuliusBrussee/caveman && claude plugin install caveman@caveman');
    expect(installScript('ponytail', 'claude')).toBe('claude plugin marketplace add DietrichGebert/ponytail && claude plugin install ponytail@ponytail');
  });

  it('installs the plugins as Gemini CLI extensions', () => {
    expect(installScript('caveman', 'gemini')).toBe('gemini extensions install https://github.com/JuliusBrussee/caveman');
  });

  it('installs the plugins as global skills with the skills CLI elsewhere, mapping agent ids to its slugs', () => {
    expect(installScript('ponytail', 'codex')).toBe("npx -y skills add DietrichGebert/ponytail --skill '*' -g -a codex -y");
    expect(installScript('caveman', 'copilot')).toContain('-a github-copilot');
    expect(installScript('caveman', 'agy')).toContain('-a antigravity');
  });

  it('installs the rtk binary when missing, then wires its hook for the agent', () => {
    const s = installScript('rtk', 'claude')!;
    expect(s.startsWith('command -v rtk')).toBe(true);
    expect(s).toContain('rtk-ai/rtk/refs/heads/master/install.sh | sh');
    expect(s.endsWith('rtk init -g --auto-patch')).toBe(true);
    expect(installScript('rtk', 'codex')!.endsWith('rtk init -g --codex')).toBe(true);
    expect(installScript('rtk', 'cursor')!.endsWith('rtk init -g --agent cursor')).toBe(true);
  });

  it('returns null where a tool has no install route', () => {
    expect(installScript('rtk', 'amp')).toBeNull();
    expect(installScript('caveman', 'shell')).toBeNull();
    expect(installScript('rtk', 'shell')).toBeNull();
  });

  it('has a route for every skill on Claude Code', () => {
    for (const s of SKILLS) expect(installScript(s.id, 'claude')).not.toBeNull();
  });
});

describe('installCmd', () => {
  it('runs the script with the daemon PATH, not a login shell that resets it, and waits for Enter so a clean exit does not close the output', () => {
    const [sh, flag, body] = installCmd('echo hi', 'Done.');
    expect([sh, flag]).toEqual(['/bin/sh', '-c']);
    expect(body).toBe("echo hi; s=$?; printf '\\n%s\\n' 'Done.'; read _; exit $s");
  });

  it('quotes single quotes in the closing message', () => {
    expect(installCmd('true', "it's done")[2]).toContain(`'it'\\''s done'`);
  });
});

describe('installTargets', () => {
  it('lists only installed agents the tool can be installed for', () => {
    const agents = [agent('claude'), agent('amp'), agent('codex', false), agent('shell')];
    expect(installTargets('rtk', agents).map((a) => a.id)).toEqual(['claude']);
    expect(installTargets('caveman', agents).map((a) => a.id)).toEqual(['claude', 'amp']);
  });
});

describe('startInstall', () => {
  it('opens a terminal running the install command, titled with the tool and agent', async () => {
    const createTerm = vi.fn(async (t: NewTerm) => ({ id: 't1', title: t.title }) as Term);
    const term = await startInstall(createTerm, 'caveman', { ...agent('claude'), name: 'Claude Code' }, 'Done.');
    expect(term.id).toBe('t1');
    const body = createTerm.mock.calls[0][0];
    expect(body.title).toBe('Caveman · Claude Code');
    expect(body.agent).toBeUndefined();
    expect(body.cmd![2].startsWith(installScript('caveman', 'claude')!)).toBe(true);
  });

  it('rejects without calling the daemon when the agent has no route', async () => {
    const createTerm = vi.fn();
    await expect(startInstall(createTerm, 'rtk', agent('amp'), 'Done.')).rejects.toThrow('no install route for rtk on amp');
    expect(createTerm).not.toHaveBeenCalled();
  });

  it('passes on a daemon error', async () => {
    const createTerm = vi.fn(async () => {
      throw new Error('daemon down');
    });
    await expect(startInstall(createTerm, 'ponytail', agent('codex'), 'Done.')).rejects.toThrow('daemon down');
  });
});
