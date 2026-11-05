export type Layout = 'phone' | 'desktop';
export type MobileTab = 'agents' | 'tasks' | 'review' | 'more';

export const PHONE_MAX_WIDTH = 720;

export interface LayoutInput {
  width: number;
  forcePhone?: boolean;
  mobileOs?: boolean;
}

export function layoutFor({ width, forcePhone = false, mobileOs = false }: LayoutInput): Layout {
  if (forcePhone) return 'phone';
  if (mobileOs) return width >= 1024 ? 'desktop' : 'phone';
  return width > 0 && width <= PHONE_MAX_WIDTH ? 'phone' : 'desktop';
}

export const KEYBOARD_MIN = 80;

export function keyboardInset(layoutHeight: number, viewportHeight: number, scale = 1): number {
  if (!(viewportHeight > 0)) return 0;
  const inset = Math.round(layoutHeight - viewportHeight * scale);
  return inset >= KEYBOARD_MIN ? inset : 0;
}

export function phoneParam(search: string): boolean {
  const v = new URLSearchParams(search).get('phone');
  return v === '1' || v === 'true';
}

export function mobileUserAgent(ua: string): boolean {
  return /Android|iPhone|iPad|iPod/i.test(ua);
}

export const MOBILE_TABS: MobileTab[] = ['agents', 'tasks', 'review', 'more'];

export function tabForView(view: 'agents' | 'kanban' | 'browser', selectedKey: string | null): MobileTab {
  if (view === 'kanban') return 'tasks';
  if (view === 'browser') return 'more';
  if (selectedKey?.startsWith('review:')) return 'review';
  return 'agents';
}

export const MIN_TOUCH = 44;

export function columnAfterSwipe(index: number, count: number, direction: -1 | 0 | 1): number {
  return Math.max(0, Math.min(count - 1, index + direction));
}
