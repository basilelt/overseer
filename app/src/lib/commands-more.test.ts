import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

const m = vi.hoisted(() => ({
  broadcastTo: vi.fn(),
  createFolder: vi.fn(),
  createGroup: vi.fn(),
  holdAll: vi.fn(),
  rescanFolders: vi.fn(),
  openReview: vi.fn(),
  openReviewBeside: vi.fn(),
  closeSideReview: vi.fn(),
  reviewTargetFor: vi.fn(),
  openPrReviews: vi.fn(),
  toggleLiveReview: vi.fn(),
  replayTour: vi.fn(),
}));

vi.mock('./wsactions', async (orig) => ({ ...(await orig<object>()), broadcastTo: m.broadcastTo, createFolder: m.createFolder, createGroup: m.createGroup, holdAll: m.holdAll, rescanFolders: m.rescanFolders }));
vi.mock('./review/open', async (orig) => ({ ...(await orig<object>()), openReview: m.openReview, openReviewBeside: m.openReviewBeside, closeSideReview: m.closeSideReview, reviewTargetFor: m.reviewTargetFor }));
vi.mock('./github/open.svelte', async (orig) => ({ ...(await orig<object>()), openPrReviews: m.openPrReviews }));
vi.mock('./git/live-actions', async (orig) => ({ ...(await orig<object>()), toggleLiveReview: m.toggleLiveReview }));
vi.mock('./tour/replay', () => ({ replayTour: m.replayTour }));

import { command } from './commands';
import { registerMore } from './commands-more';
import { diag } from './diag.svelte';
import { feedback } from './feedback.svelte';
import { organizer } from './organize.svelte';
import { recorder } from './record.svelte';
import { beside } from './review/beside.svelte';
import { app } from './state.svelte';
import { updater } from './updater.svelte';

beforeAll(() => registerMore());

afterEach(() => {
  vi.restoreAllMocks();
  for (const f of Object.values(m)) f.mockReset();
  app.open(null);
  app.view = 'agents';
  app.multi = new Set();
  beside.key = null;
});

describe('review commands', () => {
  it('opens the review of the selected line, in place or beside the agent', () => {
    m.reviewTargetFor.mockReturnValue({ line: 'l' });
    command('review').run();
    expect(m.openReview).toHaveBeenCalledWith({ line: 'l' });
    command('review-beside').run();
    expect(m.openReviewBeside).toHaveBeenCalledWith({ line: 'l' });
  });

  it('tells the user when nothing is selected to review', () => {
    const notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
    m.reviewTargetFor.mockReturnValue(null);
    command('review').run();
    expect(m.openReview).not.toHaveBeenCalled();
    expect(notify).toHaveBeenCalledWith(expect.any(String), 'error');
  });

  it('closes the side review only while one is open', () => {
    expect(command('review-close').available()).toBe(false);
    beside.key = 'line:a' as never;
    expect(command('review-close').available()).toBe(true);
    command('review-close').run();
    expect(m.closeSideReview).toHaveBeenCalled();
  });

  it('toggles the live review and opens the PR reviews', () => {
    command('live-review').run();
    command('prs').run();
    expect(m.toggleLiveReview).toHaveBeenCalled();
    expect(m.openPrReviews).toHaveBeenCalled();
  });
});

describe('host commands', () => {
  it('opens the connect dialog to switch, to add a host and to disconnect', () => {
    const open = vi.spyOn(app, 'open');
    const disconnect = vi.spyOn(app, 'disconnect').mockResolvedValue();
    command('connect').run();
    command('add-host').run();
    command('disconnect').run();
    expect(open.mock.calls).toEqual([['connect'], ['connect', 'add']]);
    expect(disconnect).toHaveBeenCalled();
  });

  it('labels the connect command after the connection state', () => {
    vi.spyOn(app, 'connected', 'get').mockReturnValue(false);
    const off = command('connect').label!();
    vi.spyOn(app, 'connected', 'get').mockReturnValue(true);
    expect(command('connect').label!()).not.toBe(off);
  });

  it('opens the ports dialog, and the forward form for a remote host', () => {
    const open = vi.spyOn(app, 'open');
    command('ports').run();
    command('forward-port').run();
    expect(open.mock.calls).toEqual([['ports'], ['ports', 'forward']]);
  });

  it('opens the skills tab of the settings', () => {
    const open = vi.spyOn(app, 'open');
    command('skills').run();
    expect(open).toHaveBeenCalledWith('settings', 'skills');
  });
});

describe('workspace commands', () => {
  it('broadcasts to, holds and resumes the current workspace', () => {
    vi.spyOn(app, 'workspaceId', 'get').mockReturnValue('w1');
    command('ws-broadcast').run();
    command('ws-hold').run();
    command('ws-resume').run();
    expect(m.broadcastTo).toHaveBeenCalledWith({ kind: 'workspace', id: 'w1' });
    expect(m.holdAll.mock.calls).toEqual([
      [{ kind: 'workspace', id: 'w1' }, 'hold'],
      [{ kind: 'workspace', id: 'w1' }, 'resume'],
    ]);
  });

  it('groups and broadcasts to the selection', () => {
    app.multi = new Set(['line:a', 'line:b']);
    command('group-new').run();
    command('sel-broadcast').run();
    expect(m.createGroup).toHaveBeenCalledWith(['line:a', 'line:b']);
    expect(m.broadcastTo).toHaveBeenCalledWith({ kind: 'items', keys: ['line:a', 'line:b'] });
  });

  it('creates folders and rescans them', () => {
    command('folder-new').run();
    command('rescan-folders').run();
    expect(m.createFolder).toHaveBeenCalled();
    expect(m.rescanFolders).toHaveBeenCalled();
  });
});

describe('tool commands', () => {
  it('starts the tour, the logs, the feedback sheet, the organizer and the recorder', () => {
    const logs = vi.spyOn(diag, 'open').mockImplementation(() => {});
    const feedbackShow = vi.spyOn(feedback, 'show').mockImplementation(() => {});
    const organize = vi.spyOn(organizer, 'show').mockImplementation(() => {});
    const setup = vi.spyOn(recorder, 'setup').mockImplementation(() => {});
    command('tour').run();
    command('logs').run();
    command('feedback').run();
    command('organize').run();
    command('record').run();
    expect(m.replayTour).toHaveBeenCalled();
    expect(logs).toHaveBeenCalledWith('live');
    expect(feedbackShow).toHaveBeenCalled();
    expect(organize).toHaveBeenCalled();
    expect(setup).toHaveBeenCalled();
  });

  it('checks for updates only where the updater is enabled and the app is not the phone', () => {
    const check = vi.spyOn(updater, 'check').mockImplementation((() => {}) as never);
    vi.spyOn(updater, 'enabled', 'get').mockReturnValue(true);
    vi.spyOn(app, 'mobile', 'get').mockReturnValue(false);
    expect(command('updates').available()).toBe(true);
    command('updates').run();
    expect(check).toHaveBeenCalledWith(true);
    vi.spyOn(app, 'mobile', 'get').mockReturnValue(true);
    expect(command('updates').available()).toBe(false);
  });
});
