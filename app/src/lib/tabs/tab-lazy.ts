// Everything only needed once a tab is renamed, moved or hovered with a cut title: one chunk, loaded on demand.
export { default as TabTitleExtra } from '../components/workspace/TabTitleExtra.svelte';
export { moveMenu, moveToWorkspace, renameItem, tabClosed, tabMenu } from './tab-edits';
export { closeCleanExits } from './autoclose';
