import { describe, expect, it } from 'vitest';
import { MOCK_QUESTION, MOCK_SCREEN } from './mock';
import { isPickerCommand, parsePicker, parsePrompt } from './prompt';

describe('prompt card', () => {
  it('maps a Claude Code permission menu to digit keys', () => {
    const p = parsePrompt(MOCK_SCREEN, 'claude')!;
    expect(p.question).toContain('Do you want to proceed?');
    expect(p.options).toEqual([
      { label: '1. Yes', keys: ['1'] },
      { label: "2. Yes, and don't ask again for cargo test commands", keys: ['2'] },
      { label: '3. No, and tell Claude what to do differently', keys: ['3'] },
    ]);
  });

  it('maps an AskUserQuestion form with tabs, descriptions, Type something and Chat about this', () => {
    const p = parsePrompt(MOCK_QUESTION, 'claude')!;
    expect(p.tabs).toEqual([
      { label: 'Epic', done: true, current: false },
      { label: 'Backing svc', done: false, current: true },
      { label: 'Seed', done: false, current: false },
      { label: 'Auth', done: false, current: false },
      { label: 'Submit', done: false, current: false },
    ]);
    expect(p.question).toBe('Where do the backing services of a preview env run?');
    expect(p.cancel).toBe(true);
    expect(p.submit).toBeUndefined();
    expect(p.options.map((o) => o.label)).toEqual(['1. Sidecars in the pod (Recommended)', '2. Shared instances', '3. Shared DB, sidecar rest', '4. Type something.', '5. Chat about this']);
    expect(p.options[0].description).toBe('Each env self-contained, data on the env PVC, seeded from a dump at first start. Simple teardown = delete namespace');
    expect(p.options[2].description).toBe('Shared Postgres with per-env DB copy, MinIO and mailpit as sidecars');
    expect(p.options.map((o) => o.keys)).toEqual([['up', 'up', 'up', 'enter'], ['up', 'up', 'enter'], ['up', 'enter'], [], ['down', 'enter']]);
    expect(p.options[3]).toMatchObject({ input: true });
    expect(p.options[4]).toMatchObject({ chat: true });
    expect(p.options.filter((o) => o.input || o.chat)).toHaveLength(2);
  });

  it('computes moves from the cursor, wherever it is', () => {
    const top = MOCK_QUESTION.replace('❯ 4. Type something.', '  4. Type something.').replace(' 1. Sidecars', '❯ 1. Sidecars');
    const p = parsePrompt(top, 'claude')!;
    expect(p.options[3].keys).toEqual(['down', 'down', 'down']);
    expect(p.options[4].keys).toEqual(['down', 'down', 'down', 'down', 'enter']);
    expect(p.options[0].keys).toEqual(['enter']);
    const chat = MOCK_QUESTION.replace('❯ 4. Type something.', '  4. Type something.').replace(' 5. Chat about this', '❯ 5. Chat about this');
    expect(parsePrompt(chat, 'claude')!.options.map((o) => o.keys)).toEqual([['up', 'up', 'up', 'up', 'enter'], ['up', 'up', 'up', 'enter'], ['up', 'up', 'enter'], ['up'], ['enter']]);
  });

  it('maps a single question without tabs and keeps typed text as the field', () => {
    const screen = ['☐ Color', '', 'Which color?', '', '  1. Red', '  2. Blue', '❯ 3. teal please', '────────', '  4. Chat about this', '', 'Enter to select · ↑/↓ to navigate · Esc to cancel'].join('\n');
    const p = parsePrompt(screen, 'claude')!;
    expect(p.tabs).toBeUndefined();
    expect(p.question).toBe('Which color?');
    expect(p.options[2]).toMatchObject({ label: '3. teal please', input: true, keys: [] });
    expect(p.options[3]).toMatchObject({ chat: true, keys: ['down', 'enter'] });
    expect(parsePrompt(screen.replace('────────\n', ''), 'claude')!.options[3]).toMatchObject({ chat: true });
  });

  it('maps a multi-select question to Space toggles and a Submit row', () => {
    const screen = [
      '←  ☒ Epic  ☒ Backing svc  ☒ Seed  ☐ Auth  ✔ Submit  →',
      '',
      'Which login methods should a preview env accept?',
      '',
      '  1. [✔] SSO',
      '         Through the cluster OIDC proxy',
      '❯ 2. [ ] Password',
      '  3. [ ] Type something',
      '',
      '     Submit',
      '',
      'Enter to select · Tab/Arrow keys to navigate · Esc to cancel',
    ].join('\n');
    const p = parsePrompt(screen, 'claude')!;
    expect(p.tabs!.find((t) => t.current)!.label).toBe('Auth');
    expect(p.options.map((o) => [o.label, o.checked, o.keys])).toEqual([
      ['1. SSO', true, ['up', 'space']],
      ['2. Password', false, ['space']],
      ['3. Type something', false, ['down']],
    ]);
    expect(p.options[0].description).toBe('Through the cluster OIDC proxy');
    expect(p.options[2].input).toBe(true);
    expect(p.submit).toEqual({ label: 'Submit', keys: ['down', 'down', 'enter'] });
  });

  it('counts the Submit row as the cursor when it is focused', () => {
    const screen = ['Pick', '', '  1. [ ] A', '  2. [✔] B', '  3. [ ] Type something', '', '❯    Next'].join('\n');
    const p = parsePrompt(screen, 'claude')!;
    expect(p.submit).toEqual({ label: 'Next', keys: ['enter'] });
    expect(p.options[0].keys).toEqual(['up', 'up', 'up', 'space']);
  });

  it('maps the review step of a multi-question form', () => {
    const screen = [
      '←  ☒ Epic  ☒ Backing svc  ☒ Seed  ☒ Auth  ✔ Submit  →',
      '────────────────────────',
      'Review your answers',
      '',
      ' ● Where do the backing services run?',
      '   → Sidecars in the pod',
      '',
      'Ready to submit your answers?',
      '',
      '❯ 1. Submit answers',
      '  2. Cancel',
    ].join('\n');
    const p = parsePrompt(screen, 'claude')!;
    expect(p.tabs!.map((t) => t.current)).toEqual([false, false, false, false, true]);
    expect(p.question).toBe('Review your answers\n● Where do the backing services run?\n→ Sidecars in the pod\nReady to submit your answers?');
    expect(p.options.map((o) => [o.label, o.keys])).toEqual([
      ['1. Submit answers', ['enter']],
      ['2. Cancel', ['down', 'enter']],
    ]);
  });

  it('maps a Codex approval menu to its shortcut letters', () => {
    const screen = ['Would you like to run the following command?', '', '  $ cargo test', '', '› 1. Yes, proceed (y)', "  2. Yes, and don't ask again for this command (a)", '  3. No, and tell Codex what to do differently (esc)', '', '  Press enter to confirm or esc to cancel'].join('\n');
    const p = parsePrompt(screen, 'codex')!;
    expect(p.options.map((o) => o.keys)).toEqual([['y'], ['a'], ['esc']]);
    expect(p.options[0].label).toBe('1. Yes, proceed');
  });

  it('uses arrows for a numbered Codex menu without shortcuts', () => {
    const screen = ['Select model', '  1. gpt-5', '› 2. gpt-5-codex', '  3. o3'].join('\n');
    expect(parsePrompt(screen, 'codex')!.options.map((o) => o.keys)).toEqual([['up', 'enter'], ['enter'], ['down', 'enter']]);
  });

  it('maps a cursor menu without numbers to arrow moves', () => {
    const screen = ['Pick a theme', '  Dark', '❯ Light', '  Solarized'].join('\n');
    const p = parsePrompt(screen, 'gemini')!;
    expect(p.question).toBe('Pick a theme');
    expect(p.options).toEqual([
      { label: 'Dark', keys: ['up', 'enter'] },
      { label: 'Light', keys: ['enter'] },
      { label: 'Solarized', keys: ['down', 'enter'] },
    ]);
  });

  it('maps letter shortcut prompts', () => {
    const screen = ['Allow command?', '  Yes (y)', '  No (n)'].join('\n');
    expect(parsePrompt(screen, 'codex')).toEqual({ question: 'Allow command?', options: [{ label: 'Yes', keys: ['y'] }, { label: 'No', keys: ['n'] }] });
  });

  it('maps a generic y/n question', () => {
    expect(parsePrompt('Overwrite file? [y/N]', 'other')!.options.map((o) => o.keys)).toEqual([['y', 'enter'], ['n', 'enter']]);
  });

  it('returns null for an idle prompt or plain output', () => {
    expect(parsePrompt('● Done.\n\n────────\n❯ \n────────\n  ? for shortcuts', 'claude')).toBeNull();
    expect(parsePrompt('compiling...\nfinished', 'claude')).toBeNull();
    expect(parsePrompt('', 'claude')).toBeNull();
  });
});

const ADVISOR = [
  '❯ /advisor',
  '',
  '────────────────────────────────────────',
  '  Advisor (experimental)',
  '',
  '  When Claude needs stronger judgment, it escalates to the advisor model for guidance, then resumes.',
  '',
  '  ❯ 1. Fable 5.1 ✔',
  '    2. Opus 5.5',
  '    3. Sonnet 5.5',
  '    4. No advisor',
  '',
  '  Recommended setup: Sonnet as the main model with Opus as the advisor.',
  '',
  '  Learn more: https://claude.com/blog/the-advisor-strategy',
  '',
  '  Enter to confirm · Esc to cancel',
].join('\n');

describe('slash command pickers', () => {
  it('maps the /advisor picker to digit keys', () => {
    const p = parsePicker(ADVISOR, 'claude')!;
    expect(p.question).toContain('advisor model');
    expect(p.options.map((o) => o.label)).toEqual(['1. Fable 5.1 ✔', '2. Opus 5.5', '3. Sonnet 5.5', '4. No advisor']);
    expect(p.options[1].keys).toEqual(['2']);
  });

  it('ignores numbered lists in agent output that no picker shows', () => {
    const reply = ['● Steps:', '  1. Build', '  2. Test', '', '────────', '❯ ', '────────', '  ? for shortcuts'].join('\n');
    expect(parsePrompt(reply, 'claude')).not.toBeNull();
    expect(parsePicker(reply, 'claude')).toBeNull();
    expect(parsePicker(reply.replace('? for shortcuts', 'esc to interrupt'), 'claude')).toBeNull();
  });

  it('probes only for a bare slash command', () => {
    expect(isPickerCommand('/advisor')).toBe(true);
    expect(isPickerCommand(' /model \n')).toBe(true);
    expect(isPickerCommand('/plugin:skill')).toBe(true);
    expect(isPickerCommand('/review 42')).toBe(false);
    expect(isPickerCommand('hello /model')).toBe(false);
    expect(isPickerCommand('/')).toBe(false);
  });
});
