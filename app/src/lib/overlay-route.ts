export type MainOverlay =
  | 'palette'
  | 'mobileConnect'
  | 'connect'
  | 'settings'
  | 'task'
  | 'taskDetail'
  | 'project'
  | 'review'
  | 'broadcast'
  | 'holdAll'
  | 'name'
  | 'workspaceNew'
  | 'profiles'
  | 'ports'
  | 'system'
  | 'usage'
  | 'notifications'
  | 'file'
  | 'switchboardImport'
  | 'uninstall'
  | 'closeTab'
  | 'shortcuts';

const PLAIN = new Set<string>(['palette', 'settings', 'task', 'taskDetail', 'project', 'review', 'broadcast', 'holdAll', 'name', 'workspaceNew', 'profiles', 'ports', 'system', 'usage', 'notifications', 'switchboardImport', 'uninstall']);

export interface OverlayState {
  overlay: string | null;
  overlayArg?: string | null;
  hasFile: boolean;
  phone: boolean;
}

export function mainOverlay(s: OverlayState): MainOverlay | null {
  const o = s.overlay;
  if (!o) return null;
  if (o === 'connect') return s.phone ? 'mobileConnect' : 'connect';
  if (o === 'file') return s.hasFile ? 'file' : null;
  if (o === 'closeTab') return s.overlayArg ? 'closeTab' : null;
  if (o === 'shortcuts') return s.phone ? null : 'shortcuts';
  return PLAIN.has(o) ? (o as MainOverlay) : null;
}
