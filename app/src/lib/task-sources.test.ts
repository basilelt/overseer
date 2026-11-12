import { describe, expect, it } from 'vitest';
import { fillFrom, filterItems } from './task-sources';
import type { SourceItem } from './types';

const gh: SourceItem = { source: 'github', kind: 'github', key: '#50', title: 'Import tasks from issues', body: ' Details \n', url: 'https://github.com/a/b/issues/50' };
const jira: SourceItem = { source: 's1', kind: 'jira', key: 'AS-12', title: 'Fix login', body: '', url: '', ticketKey: 'AS-12' };

describe('task sources', () => {
  it('filters items by key or title, ignoring case and blanks', () => {
    expect(filterItems([gh, jira], '  ')).toEqual([gh, jira]);
    expect(filterItems([gh, jira], 'as-1')).toEqual([jira]);
    expect(filterItems([gh, jira], 'IMPORT')).toEqual([gh]);
    expect(filterItems([gh, jira], 'nothing')).toEqual([]);
  });

  it('fills a GitHub issue as title, body plus link and no ticket key', () => {
    expect(fillFrom(gh)).toEqual({ title: gh.title, description: `Details\n\n${gh.url}`, ticketKey: '' });
  });

  it('fills a Jira ticket with its key when there is no body or url', () => {
    expect(fillFrom(jira)).toEqual({ title: 'Fix login', description: 'AS-12', ticketKey: 'AS-12' });
    expect(fillFrom({ ...jira, ticketKey: null }).ticketKey).toBe('');
  });

  it('carries the task type read from the ticket', () => {
    expect(fillFrom({ ...jira, taskType: 'fix' }).taskType).toBe('fix');
    expect(fillFrom({ ...jira, taskType: null })).not.toHaveProperty('taskType');
  });
});
