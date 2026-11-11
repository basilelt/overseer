export interface PanelLoaders {
  activity(id: string): unknown;
  diff(id: string): unknown;
  related(id: string): unknown;
}

export function loadPanel(id: string, repo: boolean, load: PanelLoaders): void {
  load.activity(id);
  if (!repo) return;
  load.diff(id);
  load.related(id);
}
