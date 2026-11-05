import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { app, openWindow, menuMock, tauriWindow } = vi.hoisted(() => {
  const app: Record<string, any> = {};
  const openWindow = vi.fn(async () => {});
  const menuMock = {
    setAsAppMenu: vi.fn(async () => {}),
    setAsWindowsMenuForNSApp: vi.fn(async () => {}),
    submenus: [] as { text: string; items: any[] }[],
  };
  const tauriWindow = {
    close: vi.fn(async () => {}),
    unlisten: vi.fn(),
    focus: null as ((e: { payload: boolean }) => void) | null,
  };
  return { app, openWindow, menuMock, tauriWindow };
});

vi.mock('./state.svelte', () => ({ app, NO_WORKSPACE: '' }));
vi.mock('./window', () => ({ openWindow, windowLabel: () => 'main' }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    close: tauriWindow.close,
    onFocusChanged: async (fn: (e: { payload: boolean }) => void) => {
      tauriWindow.focus = fn;
      return tauriWindow.unlisten;
    },
  }),
}));
vi.mock('@tauri-apps/api/menu', () => ({
  Submenu: {
    new: async (o: { text: string; items: any[] }) => {
      menuMock.submenus.push(o);
      return { ...o, setAsWindowsMenuForNSApp: menuMock.setAsWindowsMenuForNSApp };
    },
  },
  Menu: { new: async () => ({ setAsAppMenu: menuMock.setAsAppMenu }) },
}));

import { allowed, answerCloseTab, closeTabKey, hint, isMac, newWindow, runShortcut, startShortcuts } from './keyboard';
import { ui } from './mobile/ui.svelte';
import { DEFAULT_SETTINGS, settings } from './settings.svelte';
import { tour } from './tour/tour.svelte';
import { EMPTY_LAYOUT, forgetLayouts, layoutFromTabs, update, workspace, type Layout } from './workspace.svelte';

function freshApp() {
  return {
    overlay: null as string | null,
    view: 'agents',
    menu: null,
    mobile: false,
    connected: false,
    wsPersistent: true,
    conn: { phase: 'idle', target: { kind: 'local' } },
    sidebarOpen: true,
    panelOpen: true,
    selectedKey: null as string | null,
    items: [] as any[],
    data: { lines: [] as any[], terms: [], tasks: [], activity: {} },
    open: vi.fn((o: string | null) => {
      app.overlay = o;
    }),
    select: vi.fn((k: string) => {
      app.selectedKey = k;
    }),
    connect: vi.fn(async () => {}),
    newSession: vi.fn(async () => {}),
    openTerm: vi.fn(async () => {}),
    notify: vi.fn(),
  };
}

class FakeElement {
  constructor(private term = false) {}
  closest(sel: string) {
    return this.term && sel === '[data-terminal]' ? this : null;
  }
}

type Win = EventTarget & { __TAURI_INTERNALS__?: object };

function stubWindow(tauri = false): Win {
  const win = new EventTarget() as Win;
  if (tauri) win.__TAURI_INTERNALS__ = {};
  vi.stubGlobal('window', win);
  return win;
}

function events(win: EventTarget, type: string) {
  const seen: CustomEvent[] = [];
  win.addEventListener(type, (e) => seen.push(e as CustomEvent));
  return seen;
}

function key(over: Record<string, unknown> = {}) {
  return {
    key: 'k',
    code: 'KeyK',
    metaKey: false,
    ctrlKey: true,
    shiftKey: false,
    altKey: false,
    isComposing: false,
    target: null as unknown,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    ...over,
  };
}

function keydownListener(win: EventTarget) {
  const spy = vi.spyOn(win, 'addEventListener');
  const stop = startShortcuts();
  const call = spy.mock.calls.find((c) => c[0] === 'keydown');
  return { fire: call![1] as unknown as (e: ReturnType<typeof key>) => void, capture: call![2], stop };
}

function setLayout(l: Layout) {
  update(() => l);
}

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const savedSettings = { fontSize: settings.fontSize, ctrlWClosesTab: settings.ctrlWClosesTab, newTabShell: settings.newTabShell };
let clock = Date.UTC(2026, 0, 1);
let win: Win;

beforeEach(() => {
  vi.useFakeTimers();
  clock += 60_000;
  vi.setSystemTime(clock);
  for (const k of Object.keys(app)) delete app[k];
  Object.assign(app, freshApp());
  Object.assign(settings, { fontSize: DEFAULT_SETTINGS.fontSize, ctrlWClosesTab: false, newTabShell: false });
  workspace.layout = EMPTY_LAYOUT;
  forgetLayouts();
  ui.layout = 'desktop';
  ui.mobileOs = false;
  tour.phase = null;
  openWindow.mockReset();
  openWindow.mockResolvedValue(undefined);
  vi.stubGlobal('Element', FakeElement);
  win = stubWindow();
});

afterEach(() => {
  Object.assign(settings, savedSettings);
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('allowed', () => {
  it('only lets palette, settings and shortcuts through while an overlay is open', () => {
    app.overlay = 'task';
    expect(allowed('palette')).toBe(true);
    expect(allowed('settings')).toBe(true);
    expect(allowed('shortcuts')).toBe(true);
    expect(allowed('kanban')).toBe(false);
    expect(allowed('newTab')).toBe(false);
  });

  it('allows global shortcuts from any view', () => {
    app.view = 'kanban';
    app.menu = { x: 1 };
    expect(allowed('toggleSidebar')).toBe(true);
    expect(allowed('fontUp')).toBe(true);
    expect(allowed('closeWindow')).toBe(true);
  });

  it('allows tab and pane shortcuts only in the agents view with no context menu open', () => {
    expect(allowed('newTab')).toBe(true);
    app.menu = { x: 1 };
    expect(allowed('newTab')).toBe(false);
    app.menu = null;
    app.view = 'kanban';
    expect(allowed('closeTab')).toBe(false);
  });
});

describe('hint', () => {
  it('uses Ctrl labels off mac', () => {
    expect(isMac).toBe(false);
    expect(hint('palette')).toMatch(/ctrl/i);
  });
});

describe('runShortcut: overlays and toggles', () => {
  it('toggles the palette open and closed', () => {
    runShortcut({ id: 'palette' });
    expect(app.overlay).toBe('palette');
    runShortcut({ id: 'palette' });
    expect(app.overlay).toBeNull();
  });

  it('toggles the shortcuts sheet but always opens settings', () => {
    runShortcut({ id: 'shortcuts' });
    expect(app.overlay).toBe('shortcuts');
    runShortcut({ id: 'shortcuts' });
    expect(app.overlay).toBeNull();
    app.overlay = 'settings';
    runShortcut({ id: 'settings' });
    expect(app.open).toHaveBeenLastCalledWith('settings');
  });

  it('switches between the kanban and agents views', () => {
    runShortcut({ id: 'kanban' });
    expect(app.view).toBe('kanban');
    runShortcut({ id: 'kanban' });
    expect(app.view).toBe('agents');
  });

  it('flips the sidebar and the panel', () => {
    runShortcut({ id: 'toggleSidebar' });
    runShortcut({ id: 'togglePanel' });
    expect(app.sidebarOpen).toBe(false);
    expect(app.panelOpen).toBe(false);
  });

  it('reconnects to the current target unless a connection is already in progress', () => {
    runShortcut({ id: 'reconnect' });
    expect(app.connect).toHaveBeenCalledWith(app.conn.target);
    app.connect.mockClear();
    app.conn.phase = 'connecting';
    runShortcut({ id: 'reconnect' });
    expect(app.connect).not.toHaveBeenCalled();
  });

  it('opens the new workspace dialog only when connected to a persistent server', () => {
    runShortcut({ id: 'newWorkspace' });
    expect(app.open).not.toHaveBeenCalled();
    app.connected = true;
    app.wsPersistent = false;
    runShortcut({ id: 'newWorkspace' });
    expect(app.open).not.toHaveBeenCalled();
    app.wsPersistent = true;
    runShortcut({ id: 'newWorkspace' });
    expect(app.open).toHaveBeenCalledWith('workspaceNew');
  });
});

describe('runShortcut: font size', () => {
  it('steps the font size up and down', () => {
    settings.fontSize = 13;
    runShortcut({ id: 'fontUp' });
    expect(settings.fontSize).toBe(14);
    runShortcut({ id: 'fontDown' });
    runShortcut({ id: 'fontDown' });
    expect(settings.fontSize).toBe(12);
  });

  it('clamps between 9 and 24', () => {
    settings.fontSize = 24;
    runShortcut({ id: 'fontUp' });
    expect(settings.fontSize).toBe(24);
    settings.fontSize = 9;
    runShortcut({ id: 'fontDown' });
    expect(settings.fontSize).toBe(9);
  });

  it('resets to the default size', () => {
    settings.fontSize = 20;
    runShortcut({ id: 'fontReset' });
    expect(settings.fontSize).toBe(DEFAULT_SETTINGS.fontSize);
  });
});

describe('runShortcut: tabs', () => {
  it('opens a shell session when new tabs are shells and the app is connected', () => {
    const seen = events(win, 'sb:newtab');
    settings.newTabShell = true;
    app.connected = true;
    runShortcut({ id: 'newTab' });
    expect(app.newSession).toHaveBeenCalledWith('shell');
    expect(seen).toHaveLength(0);
  });

  it('asks the UI for a new tab otherwise', () => {
    const seen = events(win, 'sb:newtab');
    settings.newTabShell = true;
    runShortcut({ id: 'newTab' });
    expect(app.newSession).not.toHaveBeenCalled();
    expect(seen).toHaveLength(1);
  });

  it('reports a failed shell session as an error toast', async () => {
    settings.newTabShell = true;
    app.connected = true;
    app.newSession.mockRejectedValueOnce(new Error('no pty'));
    runShortcut({ id: 'newTab' });
    await flush();
    expect(app.notify).toHaveBeenCalledWith('no pty', 'error');
  });

  it('cycles to the next and previous tab, wrapping around', () => {
    setLayout(layoutFromTabs(['browser:a', 'browser:b', 'browser:c'], 'browser:c'));
    runShortcut({ id: 'nextTab' });
    expect(app.selectedKey).toBe('browser:a');
    expect(app.select).not.toHaveBeenCalled();
    setLayout(layoutFromTabs(['browser:a', 'browser:b', 'browser:c'], 'browser:a'));
    runShortcut({ id: 'prevTab' });
    expect(app.selectedKey).toBe('browser:c');
  });

  it('selects line and terminal tabs through app.select', () => {
    setLayout(layoutFromTabs(['browser:a', 'line:x', 'term:y'], 'browser:a'));
    runShortcut({ id: 'tabIndex', index: 1 });
    expect(app.select).toHaveBeenCalledWith('line:x');
    runShortcut({ id: 'tabIndex', index: 2 });
    expect(app.select).toHaveBeenCalledWith('term:y');
  });

  it('jumps to the last tab for index 9 and ignores missing indexes', () => {
    setLayout(layoutFromTabs(['browser:a', 'browser:b', 'browser:c']));
    runShortcut({ id: 'tabIndex', index: 8 });
    expect(app.selectedKey).toBe('browser:c');
    app.selectedKey = 'keep';
    runShortcut({ id: 'tabIndex', index: 5 });
    expect(app.selectedKey).toBe('keep');
  });

  it('does nothing on tab shortcuts without tabs', () => {
    app.selectedKey = 'keep';
    runShortcut({ id: 'nextTab' });
    expect(app.selectedKey).toBe('keep');
  });

  it('closes the active tab and activates its neighbour', () => {
    setLayout(layoutFromTabs(['browser:a', 'browser:b'], 'browser:a'));
    runShortcut({ id: 'closeTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:b']);
    expect(app.selectedKey).toBe('browser:b');
  });

  it('clears the selection when the last tab closes', () => {
    setLayout(layoutFromTabs(['browser:a']));
    app.selectedKey = 'browser:a';
    runShortcut({ id: 'closeTab' });
    expect(workspace.layout.panes[0].tabs).toEqual([]);
    expect(app.selectedKey).toBeNull();
  });

  it('ignores a second close within 150 ms so a key repeat does not close two tabs', () => {
    setLayout(layoutFromTabs(['browser:a', 'browser:b', 'browser:c'], 'browser:a'));
    runShortcut({ id: 'closeTab' });
    vi.advanceTimersByTime(100);
    runShortcut({ id: 'closeTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:b', 'browser:c']);
    vi.advanceTimersByTime(60);
    runShortcut({ id: 'closeTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:c']);
  });

  it('does nothing when there is no active tab to close', () => {
    app.selectedKey = 'keep';
    runShortcut({ id: 'closeTab' });
    expect(app.selectedKey).toBe('keep');
  });

  it('asks before closing a tab whose session is still running', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    runShortcut({ id: 'closeTab' });
    expect(app.open).toHaveBeenCalledWith('closeTab', 'line:a');
    expect(workspace.layout.panes[0].tabs).toEqual(['line:a', 'browser:b']);
  });

  it('asks before closing a live terminal tab', () => {
    app.items = [{ key: 'term:t', termId: 't' }];
    app.data.terms = [{ id: 't', exited: false }];
    setLayout(layoutFromTabs(['term:t']));
    runShortcut({ id: 'closeTab' });
    expect(app.open).toHaveBeenCalledWith('closeTab', 'term:t');
    expect(workspace.layout.panes[0].tabs).toEqual(['term:t']);
  });

  it('closes an exited terminal tab without asking', () => {
    app.items = [{ key: 'term:t', termId: 't' }];
    app.data.terms = [{ id: 't', exited: true }];
    setLayout(layoutFromTabs(['term:t']));
    runShortcut({ id: 'closeTab' });
    expect(app.open).not.toHaveBeenCalled();
    expect(workspace.layout.panes[0].tabs).toEqual([]);
  });

  it('closes a confirmed tab and activates its neighbour', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    closeTabKey('line:a', false);
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:b']);
    expect(app.selectedKey).toBe('browser:b');
  });

  it('closes the tab the dialog was opened for even though closing the dialog clears its argument', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    app.overlay = 'closeTab';
    app.overlayArg = 'line:a';
    vi.mocked(app.open).mockImplementation(() => {
      app.overlay = null;
      app.overlayArg = null;
    });
    answerCloseTab(false);
    expect(app.open).toHaveBeenCalledWith(null);
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:b']);
  });

  it('closes nothing when the close dialog is no longer open', () => {
    setLayout(layoutFromTabs(['line:a'], 'line:a'));
    app.overlay = null;
    app.overlayArg = 'line:a';
    answerCloseTab(true);
    expect(workspace.layout.panes[0].tabs).toEqual(['line:a']);
  });

  it('reopens the last closed tab in the focused pane', () => {
    setLayout(layoutFromTabs(['browser:a', 'browser:b'], 'browser:b'));
    runShortcut({ id: 'closeTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:a']);
    runShortcut({ id: 'reopenTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:a', 'browser:b']);
    expect(workspace.layout.panes[0].active).toBe('browser:b');
    expect(app.selectedKey).toBe('browser:b');
    expect(app.openTerm).not.toHaveBeenCalled();
  });

  it('does nothing when no closed tab can be reopened', () => {
    setLayout(layoutFromTabs(['browser:a']));
    runShortcut({ id: 'reopenTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:a']);
    expect(app.select).not.toHaveBeenCalled();
  });

  it('skips closed sessions that no longer exist', () => {
    app.items = [{ key: 'line:gone', lineId: 'gone' }];
    setLayout(layoutFromTabs(['line:gone', 'browser:a'], 'line:gone'));
    closeTabKey('line:gone', true);
    app.items = [];
    vi.advanceTimersByTime(200);
    runShortcut({ id: 'reopenTab' });
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:a']);
  });

  it('re-attaches a reopened live session that has no terminal yet', async () => {
    const item = { key: 'line:a', lineId: 'a' };
    app.items = [item];
    app.data.lines = [{ line: { id: 'a', state: 'running' }, local: { pid: 1, tmuxPane: '%1' } }];
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    closeTabKey('line:a', true);
    runShortcut({ id: 'reopenTab' });
    expect(app.select).toHaveBeenLastCalledWith('line:a');
    expect(app.openTerm).toHaveBeenCalledWith(item);
  });

  it('does not re-attach a reopened session that cannot be attached', () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [{ line: { id: 'a', state: 'held' }, local: {} }];
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    closeTabKey('line:a', true);
    runShortcut({ id: 'reopenTab' });
    expect(app.select).toHaveBeenLastCalledWith('line:a');
    expect(app.openTerm).not.toHaveBeenCalled();
  });

  it('reports a failed re-attach', async () => {
    app.items = [{ key: 'line:a', lineId: 'a' }];
    app.data.lines = [{ line: { id: 'a', state: 'running' }, local: { pid: 1, tmuxPane: '%1' } }];
    app.openTerm.mockRejectedValueOnce('tmux gone');
    setLayout(layoutFromTabs(['line:a', 'browser:b'], 'line:a'));
    closeTabKey('line:a', true);
    runShortcut({ id: 'reopenTab' });
    await flush();
    expect(app.notify).toHaveBeenCalledWith('tmux gone', 'error');
  });
});

describe('runShortcut: panes', () => {
  const twoPanes = (split: 'horizontal' | 'vertical', focused: number): Layout => ({
    split,
    ratio: 0.5,
    focused,
    panes: [
      { tabs: ['browser:a'], active: 'browser:a' },
      { tabs: ['browser:b'], active: 'browser:b' },
    ],
  });

  it('asks the UI to split right or down', () => {
    const seen = events(win, 'sb:split');
    runShortcut({ id: 'splitRight' });
    runShortcut({ id: 'splitDown' });
    expect(seen.map((e) => e.detail)).toEqual(['horizontal', 'vertical']);
  });

  it('moves focus between side by side panes and activates the pane tab', () => {
    setLayout(twoPanes('horizontal', 0));
    runShortcut({ id: 'focusRight' });
    expect(workspace.layout.focused).toBe(1);
    expect(app.selectedKey).toBe('browser:b');
    runShortcut({ id: 'focusLeft' });
    expect(workspace.layout.focused).toBe(0);
    expect(app.selectedKey).toBe('browser:a');
  });

  it('moves focus between stacked panes with up and down', () => {
    setLayout(twoPanes('vertical', 0));
    runShortcut({ id: 'focusRight' });
    expect(workspace.layout.focused).toBe(0);
    runShortcut({ id: 'focusDown' });
    expect(workspace.layout.focused).toBe(1);
    runShortcut({ id: 'focusUp' });
    expect(workspace.layout.focused).toBe(0);
  });

  it('does nothing with a single pane', () => {
    setLayout(layoutFromTabs(['browser:a']));
    const before = workspace.layout;
    runShortcut({ id: 'focusRight' });
    expect(workspace.layout).toBe(before);
    expect(app.selectedKey).toBeNull();
  });
});

describe('runShortcut: terminal actions', () => {
  it.each(['find', 'copy', 'paste'] as const)('dispatches a cancelable %s request to the focused terminal', (id) => {
    const seen = events(win, 'sb:term');
    runShortcut({ id });
    expect(seen).toHaveLength(1);
    expect(seen[0].detail).toBe(id);
    expect(seen[0].cancelable).toBe(true);
  });
});

describe('runShortcut: windows', () => {
  it('opens a new window', async () => {
    runShortcut({ id: 'newWindow' });
    await flush();
    expect(openWindow).toHaveBeenCalledTimes(1);
  });

  it('reports a window that fails to open', async () => {
    openWindow.mockRejectedValueOnce(new Error('denied'));
    await newWindow();
    expect(app.notify).toHaveBeenCalledWith('denied', 'error');
  });

  it('closes the native window only inside Tauri', async () => {
    tauriWindow.close.mockClear();
    runShortcut({ id: 'closeWindow' });
    await flush();
    expect(tauriWindow.close).not.toHaveBeenCalled();
    stubWindow(true);
    runShortcut({ id: 'closeWindow' });
    await vi.waitFor(() => expect(tauriWindow.close).toHaveBeenCalledTimes(1));
  });

  it('swallows a failing native close', async () => {
    stubWindow(true);
    tauriWindow.close.mockClear();
    tauriWindow.close.mockRejectedValueOnce(new Error('gone'));
    runShortcut({ id: 'closeWindow' });
    await vi.waitFor(() => expect(tauriWindow.close).toHaveBeenCalled());
    await flush();
    expect(app.notify).not.toHaveBeenCalled();
  });
});

describe('keydown handling', () => {
  it('listens in the capture phase and stops listening on cleanup', () => {
    const remove = vi.spyOn(win, 'removeEventListener');
    const { fire, capture, stop } = keydownListener(win);
    expect(capture).toBe(true);
    stop();
    expect(remove).toHaveBeenCalledWith('keydown', fire, true);
  });

  it('runs an allowed shortcut and consumes the event', () => {
    const { fire, stop } = keydownListener(win);
    const e = key();
    fire(e);
    expect(app.overlay).toBe('palette');
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).toHaveBeenCalled();
    stop();
  });

  it('leaves keys that are not shortcuts alone', () => {
    const { fire, stop } = keydownListener(win);
    const e = key({ ctrlKey: false, key: 'a', code: 'KeyA' });
    fire(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(app.open).not.toHaveBeenCalled();
    stop();
  });

  it.each([
    ['IME composition', () => undefined, { isComposing: true }],
    ['the mobile app', () => (app.mobile = true), {}],
    ['the phone layout', () => (ui.layout = 'phone'), {}],
    ['the guided tour', () => (tour.phase = 'tour'), {}],
  ])('ignores shortcuts during %s', (_, arrange, over) => {
    arrange();
    const { fire, stop } = keydownListener(win);
    const e = key(over);
    fire(e);
    expect(app.open).not.toHaveBeenCalled();
    expect(e.preventDefault).not.toHaveBeenCalled();
    stop();
  });

  it('swallows a disallowed close tab so the browser does not close the window', () => {
    settings.ctrlWClosesTab = true;
    app.view = 'kanban';
    setLayout(layoutFromTabs(['browser:a']));
    const { fire, stop } = keydownListener(win);
    const e = key({ key: 'w', code: 'KeyW' });
    fire(e);
    expect(e.preventDefault).toHaveBeenCalled();
    expect(e.stopPropagation).not.toHaveBeenCalled();
    expect(workspace.layout.panes[0].tabs).toEqual(['browser:a']);
    stop();
  });

  it('lets other disallowed shortcuts through untouched', () => {
    app.overlay = 'task';
    const { fire, stop } = keydownListener(win);
    const e = key({ key: 't', code: 'KeyT' });
    fire(e);
    expect(e.preventDefault).not.toHaveBeenCalled();
    expect(app.view).toBe('agents');
    stop();
  });

  it('leaves Ctrl+F to the terminal but handles Ctrl+Shift+F there', () => {
    const seen = events(win, 'sb:term');
    const { fire, stop } = keydownListener(win);
    const inTerm = new FakeElement(true);
    fire(key({ key: 'f', code: 'KeyF', target: inTerm }));
    expect(seen).toHaveLength(0);
    fire(key({ key: 'F', code: 'KeyF', shiftKey: true, target: inTerm }));
    expect(seen).toHaveLength(1);
    fire(key({ key: 'f', code: 'KeyF', target: new FakeElement(false) }));
    expect(seen).toHaveLength(2);
    stop();
  });

  it('does not install the native menu outside macOS', () => {
    stubWindow(true);
    menuMock.submenus.length = 0;
    const stop = startShortcuts();
    stop();
    expect(menuMock.submenus).toHaveLength(0);
  });
});

describe('on macOS', () => {
  async function loadMac(tauri: boolean) {
    vi.resetModules();
    vi.stubGlobal('navigator', { platform: 'MacIntel' });
    const w = stubWindow(tauri);
    menuMock.submenus.length = 0;
    menuMock.setAsAppMenu.mockClear();
    tauriWindow.unlisten.mockClear();
    tauriWindow.focus = null;
    const mod = await import('./keyboard');
    return { mod, w };
  }

  it('labels shortcuts with the command key', async () => {
    const { mod } = await loadMac(false);
    expect(mod.isMac).toBe(true);
    expect(mod.hint('palette')).toMatch(/⌘/);
  });

  it('installs the app menu, re-owns it on focus and tears it down on stop', async () => {
    const { mod } = await loadMac(true);
    const stop = mod.startShortcuts();
    await vi.waitFor(() => expect(tauriWindow.focus).not.toBeNull());
    expect(menuMock.submenus.map((s) => s.text)).toEqual(['Overseer', 'File', 'Edit', 'View', 'Window']);
    expect(menuMock.setAsAppMenu).toHaveBeenCalledTimes(1);
    tauriWindow.focus!({ payload: false });
    await flush();
    expect(menuMock.setAsAppMenu).toHaveBeenCalledTimes(1);
    tauriWindow.focus!({ payload: true });
    await flush();
    expect(menuMock.setAsAppMenu).toHaveBeenCalledTimes(2);
    stop();
    expect(tauriWindow.unlisten).toHaveBeenCalledTimes(1);
  });

  it('unsubscribes the menu focus listener when stopped before it is installed', async () => {
    const { mod } = await loadMac(true);
    const stop = mod.startShortcuts();
    stop();
    await vi.waitFor(() => expect(tauriWindow.unlisten).toHaveBeenCalledTimes(1));
  });

  it('routes the File menu Close Tab item to an open overlay first, then to the tab', async () => {
    const { mod } = await loadMac(true);
    const ws = await import('./workspace.svelte');
    const stop = mod.startShortcuts();
    await vi.waitFor(() => expect(tauriWindow.focus).not.toBeNull());
    const file = menuMock.submenus.find((s) => s.text === 'File')!;
    const close = file.items.find((i: { text: string }) => i.text === 'Close Tab');
    ws.update(() => ws.layoutFromTabs(['browser:a', 'browser:b'], 'browser:a'));
    app.overlay = 'palette';
    close.action();
    expect(app.overlay).toBeNull();
    expect(ws.workspace.layout.panes[0].tabs).toEqual(['browser:a', 'browser:b']);
    close.action();
    expect(ws.workspace.layout.panes[0].tabs).toEqual(['browser:b']);
    file.items.find((i: { text: string }) => i.text === 'New Window').action();
    await flush();
    expect(openWindow).toHaveBeenCalled();
    stop();
  });
});
