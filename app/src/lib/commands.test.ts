import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { allCommands, command, has, runShortcut } from './commands';
import { registerMore } from './commands-more';
import { tools } from './components/mobile/MobileMore.svelte';
import { recorder } from './record.svelte';
import { saveSettings, settings } from './settings.svelte';
import { SECTIONS } from './shortcut-sheet';
import type { ShortcutId } from './shortcuts';
import { app } from './state.svelte';

beforeAll(() => registerMore());

const saved = { theme: settings.theme };

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  saveSettings(saved);
  app.view = 'agents';
  app.panelOpen = true;
  app.sidebarOpen = true;
  app.open(null);
  recorder.dismiss();
});

function connect(on: boolean, persistent = true) {
  vi.spyOn(app, 'connected', 'get').mockReturnValue(on);
  vi.spyOn(app, 'wsPersistent', 'get').mockReturnValue(persistent);
}

describe('registry shape', () => {
  it('gives every command a unique id', () => {
    const ids = allCommands().map((c) => c.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('has a command for every shortcut and no two commands share one', () => {
    const shortcuts = allCommands().flatMap((c) => (c.shortcut ? [c.shortcut] : []));
    expect(new Set(shortcuts).size).toBe(shortcuts.length);
    const bound = SECTIONS.flatMap((s) => s.ids);
    expect([...shortcuts].sort()).toEqual([...bound].sort());
  });

  it('contains every command the phone More menu lists', () => {
    for (const tool of tools) expect(has(tool.id), tool.id).toBe(true);
  });

  it('throws on an unknown id', () => {
    expect(() => command('nope')).toThrow('unknown command: nope');
  });

  it('labels every command', () => {
    for (const c of allCommands()) expect(c.label(), c.id).not.toBe('');
  });
});

describe('availability', () => {
  const needConnection = ['review', 'review-beside', 'system', 'usage', 'prs', 'disconnect', 'skills', 'organize', 'rescan-folders', 'ports'];
  const always = ['browser', 'graph', 'worktrees', 'kanban', 'agents', 'connect', 'theme', 'panel', 'settings', 'logs', 'feedback', 'tour', 'palette', 'notifications', 'sidebar', 'shortcuts'];

  it.each(needConnection)('%s needs a connection', (id) => {
    connect(false);
    expect(command(id).available()).toBe(false);
    connect(true);
    expect(command(id).available()).toBe(true);
  });

  it.each(always)('%s is always available', (id) => {
    connect(false);
    expect(command(id).available()).toBe(true);
  });

  it.each(['ws-new', 'folder-new'])('%s needs a connection and persistent workspaces', (id) => {
    connect(true, false);
    expect(command(id).available()).toBe(false);
    connect(false, true);
    expect(command(id).available()).toBe(false);
    connect(true, true);
    expect(command(id).available()).toBe(true);
  });

  it('offers add-host when connected or when several hosts exist', () => {
    connect(false);
    vi.spyOn(app, 'hosts', 'get').mockReturnValue([{}] as never);
    expect(command('add-host').available()).toBe(false);
    vi.spyOn(app, 'hosts', 'get').mockReturnValue([{}, {}] as never);
    expect(command('add-host').available()).toBe(true);
    connect(true);
    vi.spyOn(app, 'hosts', 'get').mockReturnValue([{}] as never);
    expect(command('add-host').available()).toBe(true);
  });

  it('offers record only while the recorder is idle', () => {
    expect(command('record').available()).toBe(true);
    recorder.setup();
    expect(command('record').available()).toBe(false);
  });

  it('offers the workspace commands only inside a workspace', () => {
    connect(true);
    vi.spyOn(app, 'currentWorkspace', 'get').mockReturnValue(undefined as never);
    for (const id of ['ws-broadcast', 'ws-hold', 'ws-resume']) expect(command(id).available(), id).toBe(false);
    vi.spyOn(app, 'currentWorkspace', 'get').mockReturnValue({ id: 'w' } as never);
    for (const id of ['ws-broadcast', 'ws-hold', 'ws-resume']) expect(command(id).available(), id).toBe(true);
  });

  it('offers the selection commands only with a selection', () => {
    connect(true);
    app.multi = new Set();
    expect(command('group-new').available()).toBe(false);
    expect(command('sel-broadcast').available()).toBe(false);
    app.multi = new Set(['line:a']);
    expect(command('group-new').available()).toBe(true);
    expect(command('sel-broadcast').available()).toBe(true);
    app.multi = new Set();
  });

  it('offers forward-port only for a remote host', () => {
    connect(true);
    vi.spyOn(app, 'conn', 'get').mockReturnValue({ ...app.conn, target: { kind: 'local' } } as never);
    expect(command('forward-port').available()).toBe(false);
    vi.spyOn(app, 'conn', 'get').mockReturnValue({ ...app.conn, target: { kind: 'ssh', host: 'h' } } as never);
    expect(command('forward-port').available()).toBe(true);
  });
});

describe('effects', () => {
  it('opens each overlay command', () => {
    for (const [id, overlay] of [['settings', 'settings'], ['shortcuts', 'shortcuts'], ['palette', 'palette'], ['ports', 'ports'], ['system', 'system'], ['usage', 'usage'], ['notifications', 'notifications']] as const) {
      app.open(null);
      command(id).run();
      expect(app.overlay, id).toBe(overlay);
    }
  });

  it('toggles the palette and the shortcuts sheet, while run only opens them', () => {
    for (const id of ['palette', 'shortcuts']) {
      command(id).toggle!();
      expect(app.overlay).toBe(id);
      command(id).run();
      expect(app.overlay).toBe(id);
      command(id).toggle!();
      expect(app.overlay).toBeNull();
    }
  });

  it('sets the kanban view with run and toggles it with toggle', () => {
    command('kanban').run();
    command('kanban').run();
    expect(app.view).toBe('kanban');
    command('kanban').toggle!();
    expect(app.view).toBe('agents');
    command('kanban').toggle!();
    expect(app.view).toBe('kanban');
  });

  it('switches views', () => {
    command('browser').run();
    expect(app.view).toBe('browser');
    command('agents').run();
    expect(app.view).toBe('agents');
  });

  it('toggles the panel and the sidebar', () => {
    command('panel').run();
    expect(app.panelOpen).toBe(false);
    command('panel').run();
    expect(app.panelOpen).toBe(true);
    command('sidebar').run();
    expect(app.sidebarOpen).toBe(false);
  });

  it('cycles the theme', () => {
    saveSettings({ theme: 'auto' });
    command('theme').run();
    expect(settings.theme).toBe('light');
    command('theme').run();
    expect(settings.theme).toBe('dark');
    command('theme').run();
    expect(settings.theme).toBe('auto');
  });

  it('opens the new workspace dialog and the project tabs', () => {
    command('ws-new').run();
    expect(app.overlay).toBe('workspaceNew');
    const open = vi.spyOn(app, 'openProjectTab').mockImplementation(() => {});
    command('graph').run();
    command('worktrees').run();
    expect(open.mock.calls).toEqual([['graph'], ['worktrees']]);
  });

  it('asks the layout to split in the requested direction', () => {
    const win = new EventTarget();
    vi.stubGlobal('window', win);
    const seen: string[] = [];
    win.addEventListener('sb:split', (e) => seen.push((e as CustomEvent).detail));
    command('split-right').run();
    command('split-down').run();
    expect(seen).toEqual(['horizontal', 'vertical']);
  });

  it('marks the pane and tab commands that need the agents view', () => {
    expect(allCommands().filter((c) => c.agentsView).map((c) => c.id).sort()).toEqual(['split-down', 'split-right', 'tab-close', 'tab-new', 'tab-reopen']);
  });
});

describe('runShortcut', () => {
  it('toggles the palette from its shortcut', () => {
    runShortcut({ id: 'palette' });
    expect(app.overlay).toBe('palette');
    runShortcut({ id: 'palette' });
    expect(app.overlay).toBeNull();
  });

  it('toggles the kanban view from its shortcut', () => {
    runShortcut({ id: 'kanban' });
    expect(app.view).toBe('kanban');
    runShortcut({ id: 'kanban' });
    expect(app.view).toBe('agents');
  });

  it('ignores a command that is not available', () => {
    connect(false);
    runShortcut({ id: 'newWorkspace' });
    expect(app.overlay).toBeNull();
    connect(true);
    runShortcut({ id: 'newWorkspace' });
    expect(app.overlay).toBe('workspaceNew');
  });

  it('does nothing for an unbound shortcut id', () => {
    expect(() => runShortcut({ id: 'unknown' as ShortcutId })).not.toThrow();
  });
});
