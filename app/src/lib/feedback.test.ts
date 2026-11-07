import { describe, expect, it } from 'vitest';
import { attachProblem, ATTACH_ACCEPT, buildBody, buildDiagnostics, buildPrompt, findClone, issueTitle, labelsFor, newIssueUrl, wantsDiagnostics, PRIVACY_URL, redact, REPORT, repoSlug, stripAnsi, UrlWatcher, isFeedbackTerm, validate, validRepo, type Form } from './feedback';

const form = (over: Partial<Form> = {}): Form => ({ kind: 'bug', title: '', description: 'It crashes', attachments: [], diagnostics: null, ...over });
const enc = new TextEncoder();

describe('validation', () => {
  it('needs a description and a valid repo', () => {
    expect(validate(form(), 'o/r')).toBeNull();
    expect(validate(form({ description: '  \n' }), 'o/r')).toBe('description');
    expect(validate(form(), 'nope')).toBe('repo');
    expect(validate(form({ diagnostics: 'x'.repeat(70_000) }), 'o/r')).toBe('tooLong');
  });

  it('accepts only owner/name repos', () => {
    expect(validRepo('basilelt/overseer')).toBe(true);
    for (const r of ['', 'a', 'a/b/c', '-a/b', 'a/.b', 'a b/c', 'a/b;x']) expect(validRepo(r)).toBe(false);
  });
});

describe('title and labels', () => {
  it('prefixes the type and falls back to the first description line', () => {
    expect(issueTitle(form({ title: ' Tabs vanish ' }))).toBe('Bug: Tabs vanish');
    expect(issueTitle(form({ kind: 'idea', description: 'Dark sidebar\nmore text' }))).toBe('Idea: Dark sidebar');
    expect(issueTitle(form({ kind: 'other', title: 'x'.repeat(200) })).length).toBeLessThanOrEqual(90);
    expect(issueTitle(form({ title: '', description: 'Test SECRET sk-ant-api03-abcdefghijklmnop1234567890' }))).toBe('Bug: Test SECRET sk-***');
    expect(issueTitle(form({ title: 'login token=abc123def', description: 'd' }))).toBe('Bug: login token=***');
    expect(issueTitle(form({ title: `${'a'.repeat(70)} sk-ant-api03-abcdefghijklmnop1234567890` })).includes('sk-ant')).toBe(false);
  });

  it('maps types to labels', () => {
    expect(labelsFor('bug')).toEqual(['bug', 'feedback']);
    expect(labelsFor('idea')).toEqual(['enhancement', 'feedback']);
    expect(labelsFor('other')).toEqual(['feedback']);
  });

  it('includes diagnostics by default except for ideas', () => {
    expect(wantsDiagnostics('bug')).toBe(true);
    expect(wantsDiagnostics('idea')).toBe(false);
    expect(wantsDiagnostics('other')).toBe(true);
  });
});

describe('redaction', () => {
  it('masks the same secrets as the log redaction', () => {
    expect(redact('Authorization: Bearer abc.def-ghi')).toBe('Authorization: *** ***');
    expect(redact('url?token=s3cr3t&x=1')).toBe('url?token=***&x=1');
    expect(redact('{"token":"abc","name":"n"}')).toBe('{"token":"***","name":"n"}');
    expect(redact('GITHUB_TOKEN=ghp_ABCDEFGHijklmnop1234')).toBe('GITHUB_TOKEN=***');
    expect(redact('using ghp_ABCDEFGHijklmnop1234 now')).toBe('using ghp_*** now');
    expect(redact('key github_pat_11AAAAAAA0123456789_abc')).toBe('key github_pat_***');
    expect(redact('aws AKIAIOSFODNN7EXAMPLE end')).toBe('aws *** end');
    expect(redact('tmp ASIAIOSFODNN7EXAMPLE')).toBe('tmp ***');
    expect(redact('AKIA short xAKIAIOSFODNN7EXAMPLE AKIAIOSFODNN7EXAMPLEX')).toBe('AKIA short xAKIAIOSFODNN7EXAMPLE AKIAIOSFODNN7EXAMPLEX');
    expect(redact('OPENAI sk-proj-abcdefghijklmnopqrstuv')).toBe('OPENAI sk-***');
    expect(redact('password = hunter2 and more')).toBe('password = *** and more');
    expect(redact('ssh://bob:pa55@host:22/x')).toBe('ssh://bob:***@host:22/x');
    for (const plain of ['https://host/x?y=1', 'tokens: 5', 'task-sk-1 done', 'plain text']) expect(redact(plain)).toBe(plain);
  });
});

describe('diagnostics and body', () => {
  const diag = buildDiagnostics({
    appVersion: '1.2.3',
    os: 'macos',
    arch: 'aarch64',
    serverVersion: '1.2.0',
    hostOs: 'linux',
    hostArch: 'x86_64',
    mode: 'ssh',
    appLog: Array.from({ length: 300 }, (_, i) => `app ${i}`),
    serverLog: ['boot', 'auth token=abc123'],
  });

  it('keeps the last lines, the versions and masks secrets', () => {
    expect(diag).toContain('App: 1.2.3 (macos aarch64)');
    expect(diag).toContain('Server: 1.2.0 (linux x86_64)');
    expect(diag).toContain('Connection: ssh');
    expect(diag).toContain('last 200 lines');
    expect(diag).not.toContain('app 99\n');
    expect(diag).toContain('app 299');
    expect(diag).toContain('token=***');
    expect(diag).not.toContain('abc123');
  });

  it('wraps diagnostics in a details block and lists attachments', () => {
    const body = buildBody(form({ attachments: ['/h/uploads/a.png'], diagnostics: diag, description: 'Broke with password: hunter2' }));
    expect(body).toContain('**Type:** Bug');
    expect(body).toContain('- `/h/uploads/a.png`');
    expect(body).toMatch(/<details><summary>Diagnostics<\/summary>\n\n```text\n/);
    expect(body).toContain('</details>');
    expect(body).not.toContain('hunter2');
  });

  it('leaves a marker for the daemon instead of local paths when it uploads the attachments', () => {
    const body = buildBody(form({ attachments: ['/h/uploads/a.png'] }), true);
    expect(body).toContain('### Attachments\n@ATTACHMENTS@');
    expect(body).not.toContain('/h/uploads/a.png');
    expect(buildBody(form(), true)).not.toContain('@ATTACHMENTS@');
  });

  it('uses a longer fence when the logs contain backticks', () => {
    expect(buildBody(form({ diagnostics: 'a ``` b' }))).toContain('````text\na ``` b\n````');
  });

  it('includes the crash report when reporting a crash', () => {
    expect(buildDiagnostics({ appVersion: '', os: '', arch: '', serverVersion: '', hostOs: '', hostArch: '', mode: 'local', crash: 'panicked at x', appLog: [], serverLog: [] })).toContain(
      '--- Crash report ---\npanicked at x',
    );
  });
});

describe('agent prompt', () => {
  const base = { repo: 'o/r', kind: 'bug' as const, title: 'Bug: x', attachments: [], investigate: false, inClone: true };

  it('references the report file and asks for one issue with the marker line', () => {
    const p = buildPrompt(base);
    expect(p).toContain(REPORT);
    expect(p).toContain('gh issue list --repo o/r --state all --search');
    expect(p).toContain('gh issue create --repo o/r');
    expect(p).toContain('gh issue comment');
    expect(p).toContain('Steps to reproduce');
    expect(p).toContain('`bug`, `feedback`');
    expect(p).toContain('ISSUE_URL=<url>');
    expect(p).not.toContain('Likely cause');
    expect(p).not.toContain('--attach');
  });

  it('adds attachments and a read-only investigation when asked', () => {
    const p = buildPrompt({ ...base, kind: 'idea', attachments: ['/h/a.png'], investigate: true });
    expect(p).toContain('--attach <path>');
    expect(p).toContain('/h/a.png');
    expect(p).toContain('2.99');
    expect(p).toContain('Likely cause');
    expect(p).toContain('Read only');
    expect(p).not.toContain('Steps to reproduce');
    expect(p).toContain('`enhancement`');
  });

  it('never matches its own placeholder as a result', () => {
    expect(new UrlWatcher().push(enc.encode(buildPrompt(base)))).toBeNull();
  });
});

describe('clone lookup', () => {
  it('normalises GitHub remotes', () => {
    expect(repoSlug('git@github.com:basilelt/overseer.git')).toBe('basilelt/overseer');
    expect(repoSlug('https://github.com/basilelt/overseer')).toBe('basilelt/overseer');
    expect(repoSlug('ssh://git@github.com/o/r.git/')).toBe('o/r');
    expect(repoSlug('https://gitlab.com/o/r')).toBeNull();
    expect(repoSlug(undefined)).toBeNull();
  });

  it('finds a present clone of the target repo', () => {
    const projects = [
      { path: '/a', name: 'a', repo: 'git@github.com:o/other.git' },
      { path: '/gone', name: 'r', repo: 'git@github.com:O/R.git', missing: true },
      { path: '/r', name: 'r', repo: 'https://github.com/O/R.git' },
    ];
    expect(findClone(projects, 'o/r')).toBe('/r');
    expect(findClone(projects, 'x/y')).toBeUndefined();
  });
});

describe('ISSUE_URL detection', () => {
  const url = 'https://github.com/o/r/issues/42';

  it('finds the marker through ANSI codes, hyperlinks and chunk splits', () => {
    const w = new UrlWatcher();
    const out = `\x1b[2K\x1b[1mDone\x1b[0m\r\nISSUE_URL=\x1b]8;;${url}\x07${url}\x1b]8;;\x07\r\n`;
    const bytes = enc.encode(out);
    let found: string | null = null;
    for (let i = 0; i < bytes.length; i += 7) found ??= w.push(bytes.slice(i, i + 7));
    expect(found).toBe(url);
  });

  it('joins a URL wrapped over two lines', () => {
    expect(new UrlWatcher().push(enc.encode('ISSUE_URL=https://github.com/o/r/iss\r\nues/7\r\n'))).toBe('https://github.com/o/r/issues/7');
  });

  it('waits for the end of the number before reporting', () => {
    const w = new UrlWatcher();
    expect(w.push(enc.encode('ISSUE_URL=https://github.com/o/r/issues/4'))).toBeNull();
    expect(w.push(enc.encode('2\r\n'))).toBe('https://github.com/o/r/issues/42');
  });

  it('stops at text glued after the URL and joins indented wraps', () => {
    expect(new UrlWatcher().push(enc.encode(`ISSUE_URL=${url}Done`))).toBe(url);
    expect(new UrlWatcher().push(enc.encode('ISSUE_URL=https://github.com/o/r/is\r\n  sues/8\r\n'))).toBe('https://github.com/o/r/issues/8');
    expect(new UrlWatcher().push(enc.encode(`ISSUE_URL=${url}\r\n  ⎿ next`))).toBe(url);
  });

  it('finds a later marker after a marker followed by many short lines', () => {
    const noise = `ISSUE_URL=${'a\r\n'.repeat(2000)}`;
    expect(new UrlWatcher().push(enc.encode(`${noise}ISSUE_URL=${url}\r\n`))).toBe(url);
    expect(new UrlWatcher().push(enc.encode(noise))).toBeNull();
  });

  it('accepts comment links and ignores other URLs', () => {
    expect(new UrlWatcher().push(enc.encode(`ISSUE_URL=${url}#issuecomment-9 `))).toBe(`${url}#issuecomment-9`);
    expect(new UrlWatcher().push(enc.encode(`see ${url} and ISSUE_URL=https://example.com/x`))).toBeNull();
  });

  it('strips escape sequences', () => {
    expect(stripAnsi('\x1b[31mred\x1b[0m \x1b]0;title\x07ok')).toBe('red ok');
  });
});

describe('GitHub fallback', () => {
  it('prefills a new-issue URL and truncates long bodies', () => {
    const u = new URL(newIssueUrl('o/r', 'Bug: x', 'body'));
    expect(u.pathname).toBe('/o/r/issues/new');
    expect(u.searchParams.get('title')).toBe('Bug: x');
    expect(u.searchParams.get('body')).toBe('body');
    expect(new URL(newIssueUrl('o/r', 't', 'y'.repeat(20_000))).searchParams.get('body')!.length).toBeLessThan(7_000);
  });
});

describe('isFeedbackTerm', () => {
  const agent = { title: 'Feedback', cmd: ['claude', '--model', 'opus'], exited: false };
  it('recognises a running feedback agent', () => {
    expect(isFeedbackTerm(agent)).toBe(true);
  });
  it('recognises it once the daemon links it to a line', () => {
    expect(isFeedbackTerm({ ...agent, lineId: 'l1' } as never)).toBe(true);
  });
  it('ignores exited terms, tasks and plain shells', () => {
    expect(isFeedbackTerm({ ...agent, exited: true })).toBe(false);
    expect(isFeedbackTerm({ ...agent, taskId: 't1' })).toBe(false);
    expect(isFeedbackTerm({ ...agent, cmd: ['/bin/zsh'] })).toBe(false);
  });
  it('ignores other titles', () => {
    expect(isFeedbackTerm({ ...agent, title: 'claude' })).toBe(false);
    expect(isFeedbackTerm({ ...agent, title: 'Review PR #1' })).toBe(false);
  });
});

describe('privacy policy', () => {
  it('links to the policy on the default repository main branch', () => {
    expect(PRIVACY_URL).toBe('https://github.com/basilelt/overseer/blob/main/PRIVACY.md');
  });
});

describe('attachProblem', () => {
  const MB = 1024 * 1024;
  it('accepts the images and videos GitHub takes, whatever the case', () => {
    for (const n of ['a.png', 'a.JPG', 'a.jpeg', 'a.gif', 'a.webp', 'a.svg', 'a.mp4', 'a.MOV', 'a.webm']) expect(attachProblem(n, MB)).toBeNull();
  });
  it('rejects other types and names without an extension', () => {
    for (const n of ['notes.txt', 'a.zip', 'png', 'a.png.exe', '']) expect(attachProblem(n)).toBe('type');
  });
  it('caps images at 10 MB and videos at 100 MB', () => {
    expect(attachProblem('a.png', 10 * MB)).toBeNull();
    expect(attachProblem('a.png', 10 * MB + 1)).toBe('image');
    expect(attachProblem('a.mov', 100 * MB)).toBeNull();
    expect(attachProblem('a.mov', 100 * MB + 1)).toBe('video');
  });
  it('builds the picker accept list from the same extensions', () => {
    expect(ATTACH_ACCEPT).toBe('.png,.jpg,.jpeg,.gif,.webp,.svg,.mp4,.mov,.webm');
  });
});
