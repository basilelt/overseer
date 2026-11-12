<script lang="ts">
  import { app } from '../../state.svelte';
  import { preload } from '../../idle';
  import { diag } from '../../diag.svelte';
  import { feedback } from '../../feedback.svelte';
  import { organizer } from '../../organize.svelte';
  import { mainOverlay, parentOverlay, type MainOverlay } from '../../overlay-route';
  import { rec } from '../../record-state.svelte';
  import type { Component } from 'svelte';
  import { phoneOrMobile } from '../../mobile/phone.svelte';

  const close = () => app.open(null);
  const menu = preload(() => import('../ui/Menu.svelte'));
  const logsDialog = () => import('./LogsDialog.svelte');
  const feedbackSheet = () => import('./FeedbackSheet.svelte');
  const organizeDialog = () => import('./OrganizeDialog.svelte');
  const planReview = () => import('./PlanReview.svelte');
  const pickUp = () => import('./PickUp.svelte');
  const recordSetup = () => import('../record/RecordSetup.svelte');
  const recordPill = () => import('../record/RecordPill.svelte');

  type Load = () => Promise<{ default: Component<any> }>;
  const loaders: Record<MainOverlay, Load> = {
    palette: preload(() => import('./CommandPalette.svelte')),
    mobileConnect: () => import('../mobile/MobileConnect.svelte'),
    connect: () => import('./ConnectDialog.svelte'),
    settings: () => import('./SettingsDialog.svelte'),
    task: () => import('../kanban/TaskModal.svelte'),
    taskDetail: () => import('../kanban/TaskDetail.svelte'),
    project: () => import('../kanban/ProjectModal.svelte'),
    review: () => import('../kanban/ReviewDialog.svelte'),
    broadcast: () => import('./BroadcastDialog.svelte'),
    holdAll: () => import('./HoldAllDialog.svelte'),
    name: () => import('./NameDialog.svelte'),
    workspaceNew: () => import('./WorkspaceDialog.svelte'),
    profiles: () => import('./ProfilesDialog.svelte'),
    ports: () => import('../ports/PortsPanel.svelte'),
    system: () => import('../system/SystemPanel.svelte'),
    usage: () => import('../usage/UsagePanel.svelte'),
    notifications: () => import('./NotificationCenter.svelte'),
    file: () => import('./FileViewer.svelte'),
    switchboardImport: () => import('./SwitchboardImportDialog.svelte'),
    uninstall: () => import('./UninstallDialog.svelte'),
    closeTab: () => import('./CloseTabDialog.svelte'),
    shortcuts: () => import('./ShortcutsSheet.svelte'),
  };
  const main = $derived(mainOverlay({ overlay: app.overlay, overlayArg: app.overlayArg, hasFile: !!app.fileArg, phone: phoneOrMobile() }));
  function propsOf(k: MainOverlay): Record<string, unknown> {
    if (parentOverlay(k)) return { onclose: () => app.closeOverlay() };
    if (k === 'mobileConnect') return { onclose: close, initial: app.overlayArg };
    if (k === 'profiles') return { onclose: close, scope: app.overlayArg };
    if (k === 'switchboardImport') return { onclose: close, from: app.overlayArg };
    if (k === 'closeTab') return { onclose: close, tabKey: app.overlayArg };
    if (k === 'file') return { onclose: close, file: app.fileArg };
    return { onclose: close };
  }
</script>

{#if main}
  {#await loaders[main]() then { default: Overlay }}<Overlay {...propsOf(main)} />{/await}
{/if}
{#if diag.mode}
  {#await logsDialog() then { default: LogsDialog }}<LogsDialog />{/await}
{/if}
{#if rec.phase === 'setup'}
  {#await recordSetup() then { default: RecordSetup }}<RecordSetup />{/await}
{:else if rec.phase === 'recording'}
  {#await recordPill() then { default: RecordPill }}<RecordPill />{/await}
{/if}
{#if feedback.open}
  {#await feedbackSheet() then { default: FeedbackSheet }}{#key feedback.prefill}<FeedbackSheet />{/key}{/await}
{/if}
{#if organizer.phase === 'choice' || organizer.phase === 'waiting'}
  {#await organizeDialog() then { default: OrganizeDialog }}<OrganizeDialog />{/await}
{:else if organizer.phase === 'review' && organizer.proposal}
  {#await planReview() then { default: PlanReview }}{#key organizer.proposal.id}<PlanReview proposal={organizer.proposal} />{/key}{/await}
{:else if organizer.phase === 'pickup' && organizer.proposal}
  {#await pickUp() then { default: PickUp }}<PickUp items={organizer.proposal.pickUp} />{/await}
{/if}
{#if app.menu}
  {#await menu() then { default: Menu }}{#key app.menu}<Menu {...app.menu} onclose={() => (app.menu = null)} />{/key}{/await}
{/if}
