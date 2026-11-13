import { describe, expect, it } from 'vitest';
import { sheetRows } from './shortcut-sheet';

describe('sheetRows', () => {
  it('builds the sheet without unbound rows', () => {
    const linuxRows = sheetRows({ mac: false, ctrlWClosesTab: false }).flatMap((s) => s.rows.map((r) => r.id));
    expect(linuxRows).not.toContain('closeTab');
    expect(linuxRows).not.toContain('closeWindow');
    const macRows = sheetRows({ mac: true, ctrlWClosesTab: false }).flatMap((s) => s.rows);
    expect(macRows.find((r) => r.id === 'closeWindow')?.keys[0].label).toBe('⇧⌘W');
    const find = sheetRows({ mac: false, ctrlWClosesTab: true })
      .flatMap((s) => s.rows)
      .find((r) => r.id === 'find');
    expect(find?.keys).toEqual([
      { label: 'Ctrl+Shift+F', outsideTerminal: false },
      { label: 'Ctrl+F', outsideTerminal: true },
    ]);
    const paste = sheetRows({ mac: false, ctrlWClosesTab: true })
      .flatMap((s) => s.rows)
      .find((r) => r.id === 'paste');
    expect(paste?.keys.map((k) => k.label)).toEqual(['Ctrl+Shift+V', 'Shift+Insert']);
  });
});
