<script lang="ts">
  import { app } from '../../state.svelte';
  import { preload } from '../../idle';
  import Menu from '../ui/Menu.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { diag } from '../../diag.svelte';
  import { feedback } from '../../feedback.svelte';
  import { organizer } from '../../organize.svelte';

  const close = () => app.open(null);
  const commandPalette = preload(() => import('./CommandPalette.svelte'));
  const settingsDialog = () => import('./SettingsDialog.svelte');
  const connectDialog = () => import('./ConnectDialog.svelte');
  const taskModal = () => import('../kanban/TaskModal.svelte');
  const taskDetail = () => import('../kanban/TaskDetail.svelte');
  const projectModal = () => import('../kanban/ProjectModal.svelte');
  const reviewDialog = () => import('../kanban/ReviewDialog.svelte');
  const broadcastDialog = () => import('./BroadcastDialog.svelte');
  const holdAllDialog = () => import('./HoldAllDialog.svelte');
  const workspaceDialog = () => import('./WorkspaceDialog.svelte');
  const portsPanel = () => import('../ports/PortsPanel.svelte');
  const systemPanel = () => import('../system/SystemPanel.svelte');
  const usagePanel = () => import('../usage/UsagePanel.svelte');
  const mobileConnect = () => import('../mobile/MobileConnect.svelte');
  const logsDialog = () => import('./LogsDialog.svelte');
  const feedbackSheet = () => import('./FeedbackSheet.svelte');
  const profilesDialog = () => import('./ProfilesDialog.svelte');
  const shortcutsSheet = () => import('./ShortcutsSheet.svelte');
  const notificationCenter = () => import('./NotificationCenter.svelte');
  const fileViewer = () => import('./FileViewer.svelte');
  const organizeDialog = () => import('./OrganizeDialog.svelte');
  const planReview = () => import('./PlanReview.svelte');
  const pickUp = () => import('./PickUp.svelte');
  const nameDialog = () => import('./NameDialog.svelte');
  const closeTabDialog = () => import('./CloseTabDialog.svelte');
  const switchboardDialog = () => import('./SwitchboardImportDialog.svelte');
</script>

{#if app.overlay === 'palette'}
  {#await commandPalette() then { default: CommandPalette }}<CommandPalette onclose={close} />{/await}
{:else if app.overlay === 'connect' && (app.mobile || ui.layout === 'phone')}
  {#await mobileConnect() then { default: MobileConnect }}<MobileConnect onclose={close} initial={app.overlayArg} />{/await}
{:else if app.overlay === 'connect'}
  {#await connectDialog() then { default: ConnectDialog }}<ConnectDialog onclose={close} />{/await}
{:else if app.overlay === 'settings'}
  {#await settingsDialog() then { default: SettingsDialog }}<SettingsDialog onclose={close} />{/await}
{:else if app.overlay === 'task'}
  {#await taskModal() then { default: TaskModal }}<TaskModal onclose={close} />{/await}
{:else if app.overlay === 'taskDetail'}
  {#await taskDetail() then { default: TaskDetail }}<TaskDetail onclose={close} />{/await}
{:else if app.overlay === 'project'}
  {#await projectModal() then { default: ProjectModal }}<ProjectModal onclose={close} />{/await}
{:else if app.overlay === 'review'}
  {#await reviewDialog() then { default: ReviewDialog }}<ReviewDialog onclose={close} />{/await}
{:else if app.overlay === 'broadcast'}
  {#await broadcastDialog() then { default: BroadcastDialog }}<BroadcastDialog onclose={close} />{/await}
{:else if app.overlay === 'holdAll'}
  {#await holdAllDialog() then { default: HoldAllDialog }}<HoldAllDialog onclose={close} />{/await}
{:else if app.overlay === 'name'}
  {#await nameDialog() then { default: NameDialog }}<NameDialog onclose={close} />{/await}
{:else if app.overlay === 'workspaceNew'}
  {#await workspaceDialog() then { default: WorkspaceDialog }}<WorkspaceDialog onclose={close} />{/await}
{:else if app.overlay === 'profiles'}
  {#await profilesDialog() then { default: ProfilesDialog }}<ProfilesDialog onclose={close} scope={app.overlayArg} />{/await}
{:else if app.overlay === 'ports'}
  {#await portsPanel() then { default: PortsPanel }}<PortsPanel onclose={close} />{/await}
{:else if app.overlay === 'system'}
  {#await systemPanel() then { default: SystemPanel }}<SystemPanel onclose={close} />{/await}
{:else if app.overlay === 'usage'}
  {#await usagePanel() then { default: UsagePanel }}<UsagePanel onclose={close} />{/await}
{:else if app.overlay === 'notifications'}
  {#await notificationCenter() then { default: NotificationCenter }}<NotificationCenter onclose={close} />{/await}
{:else if app.overlay === 'file' && app.fileArg}
  {#await fileViewer() then { default: FileViewer }}<FileViewer onclose={close} file={app.fileArg} />{/await}
{:else if app.overlay === 'switchboardImport'}
  {#await switchboardDialog() then { default: SwitchboardImportDialog }}<SwitchboardImportDialog onclose={close} from={app.overlayArg} />{/await}
{:else if app.overlay === 'closeTab' && app.overlayArg}
  {#await closeTabDialog() then { default: CloseTabDialog }}<CloseTabDialog onclose={close} tabKey={app.overlayArg} />{/await}
{:else if app.overlay === 'shortcuts' && !app.mobile && ui.layout !== 'phone'}
  {#await shortcutsSheet() then { default: ShortcutsSheet }}<ShortcutsSheet onclose={close} />{/await}
{/if}
{#if diag.mode}
  {#await logsDialog() then { default: LogsDialog }}<LogsDialog />{/await}
{/if}
{#if feedback.open}
  {#await feedbackSheet() then { default: FeedbackSheet }}<FeedbackSheet />{/await}
{/if}
{#if organizer.phase === 'choice' || organizer.phase === 'waiting'}
  {#await organizeDialog() then { default: OrganizeDialog }}<OrganizeDialog />{/await}
{:else if organizer.phase === 'review' && organizer.proposal}
  {#await planReview() then { default: PlanReview }}{#key organizer.proposal.id}<PlanReview proposal={organizer.proposal} />{/key}{/await}
{:else if organizer.phase === 'pickup' && organizer.proposal}
  {#await pickUp() then { default: PickUp }}<PickUp items={organizer.proposal.pickUp} />{/await}
{/if}
{#if app.menu}
  <Menu {...app.menu} onclose={() => (app.menu = null)} />
{/if}
