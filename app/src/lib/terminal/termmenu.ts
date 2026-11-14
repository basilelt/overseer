import ClipboardPaste from '@lucide/svelte/icons/clipboard-paste';
import Copy from '@lucide/svelte/icons/copy';
import { t } from '../i18n.svelte';
import type { MenuItem } from '../commands/menu';

export function termMenuItems(act: { hasSelection: boolean; copy: () => void; paste: () => void; hint: (id: 'copy' | 'paste') => string }): MenuItem[] {
  return [
    { label: t('mobile.term.copy'), icon: Copy, hint: act.hint('copy'), disabled: !act.hasSelection, run: act.copy },
    { label: t('mobile.term.paste'), icon: ClipboardPaste, hint: act.hint('paste'), run: act.paste },
  ];
}
