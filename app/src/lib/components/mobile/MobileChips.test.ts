import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import { MockDaemon } from '../../daemon/mock';
import { app } from '../../state.svelte';
import MobileChips from './MobileChips.svelte';

const before = { data: app.data, key: app.selectedKey };
afterEach(() => {
  app.data = before.data;
  app.selectedKey = before.key;
});

const panelButton = (key: string | null) => {
  const d = new MockDaemon({ live: false });
  app.data = { lines: d.lines, terms: d.terms, tasks: d.tasks, activity: {} } as typeof app.data;
  app.selectedKey = key;
  return render(MobileChips).body.match(/<button[^>]*data-tour="panel"[^>]*>/)?.[0] ?? '';
};

describe('MobileChips side panel button', () => {
  it('opens the side panel of an agent session', () => {
    expect(panelButton('line:ln_03')).not.toContain('disabled');
  });

  it('opens it for a plain shell too, so its Git tab stays reachable on a phone', () => {
    expect(panelButton('term:tm_shell2')).not.toContain('disabled');
  });

  it('is disabled with nothing selected', () => {
    expect(panelButton(null)).toContain('disabled');
  });
});
