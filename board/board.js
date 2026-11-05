const STRINGS = {
  en: {
    title: 'Overseer Board',
    showOlder: 'Show older',
    filter: 'Filter: ticket, repo, branch, station',
    langToggle: 'FR',
    langTitle: 'Passer en français',
    live: 'live',
    reconnecting: 'reconnecting…',
    connecting: 'connecting…',
    local: 'local',
    activeCount: '{n} active',
    offlineSince: 'offline since {when}',
    noTicket: 'No ticket',
    groupCount: '{n} lines · {active} active',
    empty: 'No lines.',
    loading: 'Loading…',
    loadError: 'Could not load board: {error}',
    justNow: 'just now',
    minAgo: '{n}m ago',
    hAgo: '{n}h ago',
    dAgo: '{n}d ago',
    state_active: 'active',
    state_idle: 'idle',
    state_held: 'held',
    'state_hung-up': 'hung up',
    flag_needsInput: 'needs input',
    flag_pinned: 'pinned',
    flag_orphan: 'orphan',
    flag_duplicate: 'duplicate',
    flag_cwdMissing: 'missing worktree',
    act_resume: 'Resume',
    act_hold: 'Hold',
    act_close: 'Close',
    act_link: 'Link',
    act_pin: 'Pin',
    act_unpin: 'Unpin',
    act_diff: 'Diff',
    act_commit: 'Commit',
    act_stash: 'Stash',
    notActionable: 'Not actionable from this station',
    reasonCwdMissing: 'Worktree is missing',
    reasonActive: 'Line is already active',
    reasonRemote: 'Runs on another station: only resume, hold, close and link go through the API',
    confirm: 'Confirm',
    cancel: 'Cancel',
    close: 'Close',
    confirmTitle: '{action}: {line}',
    resumePlan: 'Will run {cmd} in {cwd} ({where})',
    relayed: 'Through the API on station {station}: {what}',
    stepsIntro: 'The following steps will run:',
    filesIntro: 'Files:',
    noFiles: 'No changes.',
    refusedTitle: '{action} refused',
    failedChecks: 'Failed checks:',
    linkTitle: 'Link ticket',
    linkHint: 'Ticket key (e.g. PROJ-123), or "none" to clear.',
    commitTitle: 'Commit message',
    commitHint: 'A message is required.',
    diffTitle: 'Diff: {worktree}',
    truncated: 'Output truncated.',
    status: 'Status',
    stat: 'Summary',
    noDiff: 'No changes.',
    done: '{action}: done',
    committed: 'Committed {sha}',
    stashed: 'Stashed: {ref}',
    linked: 'Linked to {key}',
    pinned: 'Pinned',
    unpinned: 'Unpinned',
    requestFailed: 'Request failed: {error}',
    ci_success: 'CI ok',
    ci_failure: 'CI failed',
    ci_pending: 'CI pending',
    ci_none: 'no CI',
    draft: 'draft',
  },
  fr: {
    title: 'Overseer Board',
    showOlder: 'Afficher les anciennes',
    filter: 'Filtrer : ticket, dépôt, branche, station',
    langToggle: 'EN',
    langTitle: 'Switch to English',
    live: 'en direct',
    reconnecting: 'reconnexion…',
    connecting: 'connexion…',
    local: 'locale',
    activeCount: '{n} actives',
    offlineSince: 'hors ligne depuis {when}',
    noTicket: 'Sans ticket',
    groupCount: '{n} lignes · {active} actives',
    empty: 'Aucune ligne.',
    loading: 'Chargement…',
    loadError: 'Impossible de charger le tableau : {error}',
    justNow: "à l'instant",
    minAgo: 'il y a {n} min',
    hAgo: 'il y a {n} h',
    dAgo: 'il y a {n} j',
    state_active: 'active',
    state_idle: 'inactive',
    state_held: 'en attente',
    'state_hung-up': 'raccrochée',
    flag_needsInput: 'saisie requise',
    flag_pinned: 'épinglée',
    flag_orphan: 'orpheline',
    flag_duplicate: 'doublon',
    flag_cwdMissing: 'worktree absent',
    act_resume: 'Reprendre',
    act_hold: 'Suspendre',
    act_close: 'Fermer',
    act_link: 'Lier',
    act_pin: 'Épingler',
    act_unpin: 'Désépingler',
    act_diff: 'Diff',
    act_commit: 'Commit',
    act_stash: 'Stash',
    notActionable: 'Aucune action possible depuis cette station',
    reasonCwdMissing: 'Le worktree est absent',
    reasonActive: 'La ligne est déjà active',
    reasonRemote: 'Tourne sur une autre station : seuls reprendre, suspendre, fermer et lier passent par l’API',
    confirm: 'Confirmer',
    cancel: 'Annuler',
    close: 'Fermer',
    confirmTitle: '{action} : {line}',
    resumePlan: 'Exécutera {cmd} dans {cwd} ({where})',
    relayed: 'Via l’API sur la station {station} : {what}',
    stepsIntro: 'Les étapes suivantes seront exécutées :',
    filesIntro: 'Fichiers :',
    noFiles: 'Aucune modification.',
    refusedTitle: '{action} refusé',
    failedChecks: 'Vérifications en échec :',
    linkTitle: 'Lier un ticket',
    linkHint: 'Clé du ticket (ex. PROJ-123), ou « none » pour retirer.',
    commitTitle: 'Message de commit',
    commitHint: 'Un message est obligatoire.',
    diffTitle: 'Diff : {worktree}',
    truncated: 'Sortie tronquée.',
    status: 'État',
    stat: 'Résumé',
    noDiff: 'Aucune modification.',
    done: '{action} : effectué',
    committed: 'Commit {sha} créé',
    stashed: 'Stash : {ref}',
    linked: 'Liée à {key}',
    pinned: 'Épinglée',
    unpinned: 'Désépinglée',
    requestFailed: 'Échec de la requête : {error}',
    ci_success: 'CI ok',
    ci_failure: 'CI en échec',
    ci_pending: 'CI en cours',
    ci_none: 'pas de CI',
    draft: 'brouillon',
  },
};

const EVENTS = ['line.created', 'line.updated', 'line.state_changed', 'line.hung_up', 'station.online', 'station.offline'];
const PR_TTL = 60000;
const params = new URLSearchParams(location.search);
const TOKEN = params.get('token');
const $ = (sel) => document.querySelector(sel);
const listEl = $('#list');
const dlg = $('#dlg');

let lang = pickLang();
let board = null;
let filter = '';
let showAll = false;
let liveState = 'connecting';
let refetchTimer = null;
const prCache = new Map();

function pickLang() {
  const q = params.get('lang');
  if (q === 'fr' || q === 'en') return q;
  try {
    const s = localStorage.sbLang;
    if (s === 'fr' || s === 'en') return s;
  } catch {}
  return 'en';
}

function t(key, vars) {
  const s = STRINGS[lang][key] ?? STRINGS.en[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (_, k) => (vars && vars[k] != null ? String(vars[k]) : ''));
}

function esc(v) {
  return String(v ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

function api(path) {
  if (!TOKEN) return path;
  return path + (path.includes('?') ? '&' : '?') + 'token=' + encodeURIComponent(TOKEN);
}

async function call(method, path, body) {
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(api(path), opts);
  let data = {};
  try { data = await res.json(); } catch {}
  return { ok: res.ok, status: res.status, data: data || {} };
}

function rel(iso) {
  const ms = Date.now() - new Date(iso).getTime();
  if (!iso || Number.isNaN(ms)) return '';
  const m = Math.floor(ms / 60000);
  if (m < 1) return t('justNow');
  if (m < 60) return t('minAgo', { n: m });
  if (m < 1440) return t('hAgo', { n: Math.floor(m / 60) });
  return t('dAgo', { n: Math.floor(m / 1440) });
}

const shortRepo = (repo) => String(repo || '').split('/').filter(Boolean).pop() || '';

function shortPath(p) {
  const s = String(p || '').replace(/^\/home\/[^/]+/, '~');
  return s.length > 42 ? '…' + s.slice(-41) : s;
}

function applyStatic() {
  document.documentElement.lang = lang;
  document.querySelectorAll('[data-i18n]').forEach((el) => { el.textContent = t(el.dataset.i18n); });
  $('#filter').placeholder = t('filter');
  $('#lang').textContent = t('langToggle');
  $('#lang').title = t('langTitle');
  setLive(liveState);
}

function setLive(state) {
  liveState = state;
  const el = $('#live');
  el.className = 'live is-' + state;
  el.textContent = t(state);
}

function renderStations() {
  $('#stations').innerHTML = (board?.stations || []).map((s) => {
    const cls = ['chip', s.online ? '' : 'offline', s.local ? 'local' : ''].join(' ');
    const extra = s.online ? '' : ` <small>${esc(t('offlineSince', { when: rel(s.lastSeenAt) }))}</small>`;
    return `<span class="${cls}" title="${esc(t('activeCount', { n: s.activeCount ?? 0 }))}">${esc(s.stationId)}`
      + `${s.local ? ` <em>${esc(t('local'))}</em>` : ''} <b>${esc(s.activeCount ?? 0)}</b>${extra}</span>`;
  }).join('');
}

function matches(e) {
  if (!filter) return true;
  const l = e.line;
  return [l.ticketKey, l.repo, l.branch, l.stationId].join(' ').toLowerCase().includes(filter);
}

function render() {
  renderStations();
  if (!board) return;
  const groups = new Map();
  for (const e of (board.lines || []).filter(matches)) {
    const k = e.line.ticketKey || '';
    if (!groups.has(k)) groups.set(k, []);
    groups.get(k).push(e);
  }
  const keys = [...groups.keys()].filter(Boolean).sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  if (groups.has('')) keys.push('');
  if (!keys.length) {
    listEl.innerHTML = `<p class="empty">${esc(t('empty'))}</p>`;
    return;
  }
  listEl.innerHTML = keys.map((k) => {
    const items = groups.get(k);
    const active = items.filter((e) => e.line.state === 'active').length;
    return `<section class="group"><h2>${esc(k || t('noTicket'))} <small>${esc(t('groupCount', { n: items.length, active }))}</small></h2>`
      + items.map(row).join('') + '</section>';
  }).join('');
}

function prKey(l) {
  return l.repo + '\n' + l.branch;
}

function prHtml(entry) {
  if (!entry || !('at' in entry)) return '';
  const pr = entry.pr;
  if (!pr) return '<span class="muted">—</span>';
  const ci = pr.ci || 'none';
  return `<a href="${esc(pr.url)}" target="_blank" rel="noopener" class="pr ${esc(String(pr.state).toLowerCase())}" title="${esc(pr.title)}">#${esc(pr.number)}</a>`
    + (pr.isDraft ? ` <span class="flag">${esc(t('draft'))}</span>` : '')
    + ` <span class="ci ci-${esc(ci)}">${esc(t('ci_' + ci))}</span>`;
}

function envHtml(l) {
  if (!l.envRef) return '';
  if (!board.yardUrl) return `<span class="env">${esc(l.envRef)}</span>`;
  return `<a class="env" href="${esc(board.yardUrl + '/envs/' + encodeURIComponent(l.envRef))}" target="_blank" rel="noopener">${esc(l.envRef)}</a>`;
}

function row(e) {
  const l = e.line;
  const loc = e.local || {};
  const flags = ['needsInput', 'pinned', 'orphan', 'duplicate', 'cwdMissing']
    .filter((f) => loc[f])
    .map((f) => `<span class="flag flag-${f}">${esc(t('flag_' + f))}</span>`).join('');
  const blocked = e.actionable ? '' : (e.reason || t('notActionable'));
  const why = (action) => {
    if (blocked) return blocked;
    if (e.remote && ['pin', 'diff', 'commit', 'stash'].includes(action)) return t('reasonRemote');
    if (loc.cwdMissing && ['diff', 'commit', 'stash'].includes(action)) return t('reasonCwdMissing');
    if (action === 'resume' && l.state === 'active') return t('reasonActive');
    return '';
  };
  const btn = (action, label) => {
    const r = why(action);
    return `<button type="button" data-action="${action}" data-id="${esc(l.id)}"${r ? ` disabled title="${esc(r)}"` : ''}>${esc(t('act_' + label))}</button>`;
  };
  const cached = l.branch ? prCache.get(prKey(l)) : null;
  return `<article class="line${e.actionable ? '' : ' blocked'}">
  <div class="top">
    <span class="state state-${esc(l.state)}">${esc(t('state_' + l.state))}</span>${flags}
    <span class="agent">${esc(l.agent)}</span>
    <span class="muted">${esc(l.stationId)}</span>
    ${loc.title ? `<span class="ltitle">${esc(loc.title)}</span>` : ''}
    <span class="when muted" title="${esc(l.lastActivityAt)}">${esc(rel(l.lastActivityAt))}</span>
  </div>
  <div class="meta">
    <span class="repo" title="${esc(l.repo)}">${esc(shortRepo(l.repo))}</span>
    ${l.branch ? `<code class="branch">${esc(l.branch)}</code>` : ''}
    <span class="path muted" title="${esc(l.worktreePath)}">${esc(shortPath(l.worktreePath))}</span>
    ${l.branch ? `<span class="prcell" data-pr="${esc(prKey(l))}">${cached ? prHtml(cached) : ''}</span>` : ''}
    ${envHtml(l)}
  </div>
  <div class="actions">
    ${btn('resume', 'resume')}${btn('hold', 'hold')}${btn('close', 'close')}${btn('link', 'link')}
    ${btn('pin', loc.pinned ? 'unpin' : 'pin')}${btn('diff', 'diff')}${btn('commit', 'commit')}${btn('stash', 'stash')}
  </div>
  ${blocked ? `<p class="reason">${esc(blocked)}</p>` : ''}
</article>`;
}

function fillPr(key) {
  const html = prHtml(prCache.get(key));
  listEl.querySelectorAll('[data-pr]').forEach((el) => { if (el.dataset.pr === key) el.innerHTML = html; });
}

function loadPrs() {
  const now = Date.now();
  const seen = new Set();
  for (const e of board?.lines || []) {
    const l = e.line;
    if (!l.branch) continue;
    const key = prKey(l);
    const c = prCache.get(key);
    if (seen.has(key) || (c && (c.pending || now - c.at < PR_TTL))) continue;
    seen.add(key);
    prCache.set(key, { ...c, pending: true });
    call('GET', `/v1/pr?repo=${encodeURIComponent(l.repo)}&branch=${encodeURIComponent(l.branch)}`)
      .then((r) => ({ pr: r.ok && !r.data.error ? r.data.pr : null }))
      .catch(() => ({ pr: null }))
      .then((v) => { prCache.set(key, { at: Date.now(), pr: v.pr }); fillPr(key); });
  }
}

async function load() {
  try {
    const r = await call('GET', '/v1/board' + (showAll ? '?all=true' : ''));
    if (!r.ok) throw new Error(r.data.error || 'HTTP ' + r.status);
    board = r.data;
    render();
    loadPrs();
  } catch (err) {
    if (board) toast(t('loadError', { error: err.message }), true);
    else listEl.innerHTML = `<p class="empty">${esc(t('loadError', { error: err.message }))}</p>`;
  }
}

function scheduleLoad() {
  clearTimeout(refetchTimer);
  refetchTimer = setTimeout(load, 250);
}

function connect() {
  const es = new EventSource(api('/v1/events'));
  es.onopen = () => setLive('live');
  es.onerror = () => {
    setLive('reconnecting');
    if (es.readyState === EventSource.CLOSED) setTimeout(connect, 5000);
  };
  es.onmessage = scheduleLoad;
  EVENTS.forEach((name) => es.addEventListener(name, scheduleLoad));
}

let toastTimer = null;
function toast(msg, isError) {
  const el = $('#toast');
  el.textContent = msg;
  el.className = 'show' + (isError ? ' error' : '');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.className = ''; }, 4000);
}

function modal({ title, body, ok }) {
  return new Promise((resolve) => {
    $('#dlgTitle').textContent = title;
    const bodyEl = $('#dlgBody');
    if (typeof body === 'string') bodyEl.innerHTML = body;
    else bodyEl.replaceChildren(body);
    const okBtn = $('#dlgOk');
    okBtn.hidden = !ok;
    okBtn.textContent = ok || '';
    $('#dlgCancel').textContent = ok ? t('cancel') : t('close');
    dlg.returnValue = '';
    dlg.onclose = () => {
      const input = bodyEl.querySelector('[name=value]');
      resolve(dlg.returnValue === 'ok' ? (input ? input.value.trim() : true) : null);
    };
    dlg.showModal();
    bodyEl.querySelector('[name=value]')?.focus();
  });
}

const ul = (items) => `<ul class="items">${items.map((i) => `<li><code>${esc(i)}</code></li>`).join('')}</ul>`;
const linePath = (id) => '/v1/lines/' + encodeURIComponent(id);
const lineLabel = (e) => e ? (e.line.ticketKey || shortRepo(e.line.repo)) + ' ' + (e.line.branch || '') : '';

function refuse(action, data, fails = []) {
  const checks = fails.length
    ? `<p>${esc(t('failedChecks'))}</p><ul class="items">${fails.map(([name, p]) =>
      `<li><b>${esc(name)}</b>${p.detail ? ': ' + esc(p.detail) : ''}</li>`).join('')}</ul>`
    : '';
  const error = data && data.error ? `<p class="error">${esc(data.error)}</p>` : '';
  return modal({ title: t('refusedTitle', { action: t('act_' + action) }), body: error + checks });
}

function confirmStep(action, e, html) {
  return modal({ title: t('confirmTitle', { action: t('act_' + action), line: lineLabel(e) }), body: html, ok: t('confirm') });
}

const filesHtml = (files) => files && files.length ? `<p>${esc(t('filesIntro'))}</p>${ul(files)}` : `<p>${esc(t('noFiles'))}</p>`;

const ACTIONS = {
  async resume(id, e) {
    const r = await call('POST', linePath(id) + '/resume');
    if (!r.ok) return refuse('resume', r.data);
    const p = r.data;
    const plan = p.relayed ? t('relayed', { station: e?.line.stationId || '', what: p.where })
      : t('resumePlan', { cmd: (p.argv || []).join(' '), cwd: p.cwd, where: p.where });
    if (!await confirmStep('resume', e, `<p><code>${esc(plan)}</code></p>`)) return;
    const x = await call('POST', linePath(id) + '/resume?dryRun=false');
    if (!x.ok) return refuse('resume', x.data);
    toast(t('done', { action: t('act_resume') }));
  },
  hold: (id, e) => lifecycle('hold', id, e),
  close: (id, e) => lifecycle('close', id, e),
  async link(id, e) {
    const body = `<p>${esc(t('linkHint'))}</p><input name="value" required placeholder="PROJ-123" value="${esc(e?.line.ticketKey || '')}">`;
    const key = await modal({ title: t('linkTitle'), body, ok: t('confirm') });
    if (!key) return;
    const r = await call('POST', linePath(id) + '/link', { ticketKey: key });
    if (!r.ok) return refuse('link', r.data);
    toast(t('linked', { key: r.data.ticketKey || '—' }));
  },
  async pin(id, e) {
    const pinned = !(e?.local?.pinned);
    const r = await call('PUT', linePath(id) + '/pin', { pinned });
    if (!r.ok) return refuse(pinned ? 'pin' : 'unpin', r.data);
    toast(t(pinned ? 'pinned' : 'unpinned'));
  },
  async diff(id) {
    const r = await call('GET', linePath(id) + '/diff');
    if (!r.ok) return refuse('diff', r.data);
    await modal({ title: t('diffTitle', { worktree: r.data.worktree || '' }), body: diffNode(r.data) });
    return 'readonly';
  },
  async commit(id, e) {
    const body = `<p>${esc(t('commitHint'))}</p><textarea name="value" required rows="3"></textarea>`;
    const message = await modal({ title: t('commitTitle'), body, ok: t('act_commit') });
    if (!message) return;
    const r = await call('POST', linePath(id) + '/commit', { message });
    if (!r.ok) return refuse('commit', r.data);
    if (!await confirmStep('commit', e, filesHtml(r.data.files))) return;
    const x = await call('POST', linePath(id) + '/commit?dryRun=false', { message });
    if (!x.ok) return refuse('commit', x.data);
    toast(t('committed', { sha: String(x.data.commit || '').slice(0, 12) }));
  },
  async stash(id, e) {
    const r = await call('POST', linePath(id) + '/stash');
    if (!r.ok) return refuse('stash', r.data);
    if (!await confirmStep('stash', e, filesHtml(r.data.files))) return;
    const x = await call('POST', linePath(id) + '/stash?dryRun=false');
    if (!x.ok) return refuse('stash', x.data);
    toast(t('stashed', { ref: x.data.stash || '' }));
  },
};

async function lifecycle(action, id, e) {
  const failed = (d) => Object.entries(d.predicates || {}).filter(([, p]) => p && !p.ok);
  const r = await call('POST', `${linePath(id)}/${action}`);
  if (!r.ok || r.data.error || failed(r.data).length) return refuse(action, r.data, failed(r.data));
  const steps = r.data.steps || [];
  if (!await confirmStep(action, e, `<p>${esc(t('stepsIntro'))}</p>${ul(steps)}`)) return;
  const x = await call('POST', `${linePath(id)}/${action}?dryRun=false`);
  if (!x.ok || x.data.error || failed(x.data).length) return refuse(action, x.data, failed(x.data));
  toast(t('done', { action: t('act_' + action) }));
}

function diffNode(d) {
  const wrap = document.createElement('div');
  const section = (label, text, colour) => {
    if (!text) return;
    const h = document.createElement('h3');
    h.textContent = label;
    const pre = document.createElement('pre');
    pre.className = 'diff';
    for (const line of String(text).split('\n')) {
      const span = document.createElement('span');
      if (colour) {
        if (line.startsWith('@@')) span.className = 'hunk';
        else if (line.startsWith('+') && !line.startsWith('+++')) span.className = 'add';
        else if (line.startsWith('-') && !line.startsWith('---')) span.className = 'del';
      }
      span.textContent = line + '\n';
      pre.append(span);
    }
    wrap.append(h, pre);
  };
  section(t('status'), d.status);
  section(t('stat'), d.stat);
  section(t('act_diff'), d.diff, true);
  if (!d.status && !d.diff) wrap.append(Object.assign(document.createElement('p'), { textContent: t('noDiff') }));
  if (d.truncated) wrap.append(Object.assign(document.createElement('p'), { className: 'muted', textContent: t('truncated') }));
  return wrap;
}

listEl.addEventListener('click', async (ev) => {
  const b = ev.target.closest('button[data-action]');
  if (!b || b.disabled) return;
  const id = b.dataset.id;
  const entry = board?.lines?.find((x) => x.line.id === id);
  try {
    const res = await ACTIONS[b.dataset.action](id, entry);
    if (res !== 'readonly') load();
  } catch (err) {
    toast(t('requestFailed', { error: err.message }), true);
  }
});

$('#filter').addEventListener('input', (ev) => {
  filter = ev.target.value.trim().toLowerCase();
  render();
});

$('#showAll').addEventListener('change', (ev) => {
  showAll = ev.target.checked;
  load();
});

$('#lang').addEventListener('click', () => {
  lang = lang === 'en' ? 'fr' : 'en';
  try { localStorage.sbLang = lang; } catch {}
  applyStatic();
  render();
});

applyStatic();
listEl.innerHTML = `<p class="empty">${esc(t('loading'))}</p>`;
load();
connect();
setInterval(load, 30000);
