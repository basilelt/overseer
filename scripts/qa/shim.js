(() => {
  const PORT = '__QA_PORT__';
  // The extra hosts of a window (slots) reach a second sandbox daemon when QA_SLOT_PORT is set.
  const SLOT_PORT = '__QA_SLOT_PORT__';
  const hostsBy = new Map();
  const H = (slot) => {
    const k = slot || '';
    let h = hostsBy.get(k);
    if (!h) {
      const port = k && !SLOT_PORT.startsWith('__') ? SLOT_PORT : PORT;
      h = { base: 'http://127.0.0.1:' + port, ws: 'ws://127.0.0.1:' + port, up: Promise.resolve(), wake: null };
      hostsBy.set(k, h);
    }
    return h;
  };
  const cbs = new Map(); let nextCb = 1, nextId = 1;
  const subs = new Map(), terms = new Map();
  const idx = new Map();
  const send = (ch, message) => { if (!ch) return; const id = ch.id; const i = idx.get(id) ?? 0; idx.set(id, i + 1); const f = cbs.get(id); if (f) f({ index: i, message }); };
  window.__qaLog = [];
  const log = (...a) => window.__qaLog.push(a.map(String).join(' '));
  const watches = new Map();
  window.__qaWindows = [];
  async function alive(slot) {
    try { const r = await fetch(H(slot).base + '/v1/version', { signal: AbortSignal.timeout(5000) }); return r.ok ? ((await r.json()).version ?? '') : null; } catch { return null; }
  }
  function monitor(ch, target, slot) {
    const me = { stop: false }; watches.set(slot ?? '', me);
    (async () => {
      for (;;) {
        await new Promise((r) => setTimeout(r, 5000));
        if (me.stop) return;
        if ((await alive(slot)) !== null) continue;
        const h = H(slot); h.up = new Promise((r) => (h.wake = r));
        for (let attempt = 0; !me.stop; attempt++) {
          send(ch, { phase: 'reconnecting', target, attempt, detail: 'the daemon stopped answering' });
          await new Promise((r) => setTimeout(r, 1000));
          const version = await alive(slot);
          if (version !== null) { send(ch, { phase: 'connected', target, version }); break; }
        }
        h.wake?.(); h.wake = null;
      }
    })();
  }
  async function req(method, path, body, slot) {
    const h = H(slot);
    await h.up;
    const r = await fetch(h.base + path, { method, headers: body == null ? {} : { 'Content-Type': 'application/json' }, body: body == null ? undefined : JSON.stringify(body) });
    const t = await r.text(); let b = null; try { b = t ? JSON.parse(t) : null; } catch { b = t; }
    return { status: r.status, body: b };
  }
  function events(ch, slot) {
    const id = nextId++; const ac = new AbortController(); subs.set(id, ac);
    let last = null;
    (async () => {
      while (!ac.signal.aborted) {
        try {
          await H(slot).up; const r = await fetch(H(slot).base + '/v1/events', { headers: last ? { Accept: 'text/event-stream', 'Last-Event-ID': last } : { Accept: 'text/event-stream' }, signal: ac.signal });
          const rd = r.body.getReader(); const dec = new TextDecoder(); let buf = '';
          for (;;) {
            const { value, done } = await rd.read(); if (done) break;
            buf += dec.decode(value, { stream: true });
            let k; while ((k = buf.indexOf('\n\n')) >= 0) {
              const block = buf.slice(0, k); buf = buf.slice(k + 2);
              let ev = '', eid = null; const data = [];
              for (const ln of block.split('\n')) { if (ln.startsWith('event:')) ev = ln.slice(6).trim(); else if (ln.startsWith('data:')) data.push(ln.slice(5).replace(/^ /, '')); else if (ln.startsWith('id:')) eid = ln.slice(3).trim(); }
              if (eid) last = eid;
              if (!data.length) continue;
              try { const v = JSON.parse(data.join('\n')); if (v && typeof v === 'object') { if (!('type' in v) && ev) v.type = ev; if (!('id' in v) && eid) v.id = eid; send(ch, v); } } catch {}
            }
          }
        } catch (e) { if (ac.signal.aborted) return; log('events err', e); }
        await new Promise((r) => setTimeout(r, 1000));
      }
    })();
    return id;
  }
  function attach(termId, ch, slot) {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(H(slot).ws + '/v1/terms/' + encodeURIComponent(termId) + '/io'); ws.binaryType = 'arraybuffer';
      const id = nextId++; terms.set(id, ws);
      ws.onopen = () => resolve(id);
      ws.onerror = (e) => { log('ws err', termId); reject('terminal attach failed'); };
      ws.onmessage = (m) => { if (m.data instanceof ArrayBuffer) send(ch, Array.from(new Uint8Array(m.data))); };
      ws.onclose = () => send(ch, []);
    });
  }
  const MAX_UPLOAD = 50 << 20, pastes = new Map();
  const pctDecode = (v) => { const b = new TextEncoder().encode(v), out = []; for (let i = 0; i < b.length; i++) { const hex = String.fromCharCode(b[i + 1], b[i + 2]); if (b[i] === 37 && /^[0-9a-fA-F]{2}$/.test(hex)) { out.push(parseInt(hex, 16)); i += 2; } else out.push(b[i]); } return new TextDecoder().decode(Uint8Array.from(out)); };
  const pasteName = (raw) => { const clean = [...raw.split(/[\\/]/).pop()].map((c) => (/[\p{Alphabetic}\p{N}.\-_]/u.test(c) ? c : '_')).slice(0, 80).join('').replace(/^\.+/, ''); return clean || 'paste.png'; };
  const bytesOf = (b) => b instanceof Uint8Array ? b : b instanceof ArrayBuffer ? new Uint8Array(b) : Array.isArray(b) ? Uint8Array.from(b) : null;
  const handlers = {
    save_paste: (a, o) => {
      const bytes = bytesOf(a); if (!bytes) throw 'expected bytes';
      if (bytes.length > MAX_UPLOAD) throw 'larger than the 50 MiB upload limit';
      const path = '/tmp/overseer-paste/' + Date.now() + '-' + pasteName(pctDecode((o?.headers ?? {})['x-name'] ?? ''));
      pastes.set(path, bytes);
      return path;
    },
    upload_file: async (a) => {
      const bytes = pastes.get(a.path);
      if (!bytes) throw a.path + ': No such file or directory (os error 2)';
      send(a.onProgress, { sent: 0, total: bytes.length });
      const name = a.path.split('/').pop();
      const r = await fetch(H(a.slot).base + '/v1/uploads?name=' + encodeURIComponent(name), { method: 'POST', body: bytes });
      const t = await r.text(); let b = null; try { b = t ? JSON.parse(t) : null; } catch {}
      if (!r.ok) throw (b && typeof b.error === 'string' && b.error) || 'upload failed: HTTP ' + r.status;
      send(a.onProgress, { sent: bytes.length, total: bytes.length });
      return b;
    },
    daemon_request: (a) => req(a.method, a.path, a.body, a.slot),
    daemon_bytes: async (a) => { const r = await fetch(H(a.slot).base + a.path); if (!r.ok) { const t = await r.text(); let e = `HTTP ${r.status}`; try { e = JSON.parse(t).error || e; } catch {} throw { status: r.status, error: e }; } send(a.onData, Array.from(new Uint8Array(await r.arrayBuffer()))); },
    daemon_events: (a) => events(a.onEvent, a.slot),
    daemon_events_stop: (a) => { subs.get(a.sub)?.abort(); subs.delete(a.sub); },
    term_attach: (a) => attach(a.termId, a.onData, a.slot),
    term_write: (a) => { terms.get(a.attach)?.send(new TextEncoder().encode(a.data)); },
    term_resize: (a) => { terms.get(a.attach)?.send(JSON.stringify({ resize: { cols: a.cols, rows: a.rows }, client: 'desktop' })); },
    term_background: () => null,
    diag_tail: () => ({ path: 'app.log', lines: ['qa line'] }),
    diag_info: () => null,
    update_status: () => ({ current: '0.0.0-qa', kind: 'package', releasesUrl: 'https://github.com/basilelt/overseer/releases' }),
    update_check: () => { throw 'Updates are not available in the sandbox'; },
    ports_watch: (a) => { const id = nextId++; const tick = async () => { try { const r = await fetch(H(a.slot).base + '/v1/ports'); send(a.onState, { remote: false, detected: await r.json(), forwards: [] }); } catch {} }; tick(); subs.set(id, { abort: () => clearInterval(iv) }); const iv = setInterval(tick, 1500); return id; },
    ports_watch_stop: (a) => { subs.get(a.sub)?.abort(); subs.delete(a.sub); },
    ports_configure: () => null,
    app_platform: () => { const os = new URLSearchParams(location.search).get('qaos'); return os ? { os, mobile: os === 'android' || os === 'ios' } : { os: 'linux', mobile: false }; },
    'plugin:clipboard-manager|write_text': (a) => { window.__qaClip = a.text; return null; },
    'plugin:clipboard-manager|read_text': () => window.__qaClip ?? '',
    browsers_list: () => [{ id: 'qa-browser', name: 'QA Browser' }],
    browser_open_with: (a) => { (window.__qaOpened ??= []).push({ via: a.id, url: a.url }); return null; },
    open_external: (a) => { (window.__qaOpened ??= []).push({ via: 'system', url: a.url }); return null; },
    local_network_status: (a) => ({ status: 'notApplicable', host: a?.host ?? null, addresses: [], private: false }),
    diag_pong: () => null,
    diag_previous: () => null,
    mobile_system_dark: () => null,
    mobile_system_bars: () => null,
    'plugin:window|set_badge_count': () => null,
    'plugin:window|theme': () => (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'),
    'plugin:app|register_listener': () => null,
    'plugin:notification|register_listener': () => null,
    save_download: (a, o) => { const bytes = bytesOf(a); if (!bytes) throw 'expected bytes'; window.__qaDownload = { name: pasteName(pctDecode(o?.headers?.['x-name'] ?? '')), size: bytes.length }; return '/home/qa/Downloads/' + window.__qaDownload.name; },
    conn_connect: async (a) => { const w = watches.get(a.slot ?? ''); if (w) w.stop = true; log('conn_connect', a.slot ?? '', JSON.stringify(a.target)); send(a.onState, { phase: 'connected', target: a.target, version: (await alive(a.slot)) ?? '0.2.0' }); monitor(a.onState, a.target, a.slot); },
    conn_disconnect: (a) => { const slot = a?.slot ?? ''; log('conn_disconnect', slot); const w = watches.get(slot); if (w) w.stop = true; watches.delete(slot); const h = H(slot); h.wake?.(); h.wake = null; h.up = Promise.resolve(); return null; },
    window_new: (a) => { window.__qaWindows.push(a?.connect ?? null); return 'w-' + (window.__qaWindows.length + 1); },
    window_handoff: () => null,
    conn_drop_slots: () => { for (const [slot, w] of watches) if (slot) { w.stop = true; watches.delete(slot); } return null; },
    ssh_hosts: () => [],
    direct_hosts: () => [],
    direct_device_key: () => ({ publicKey: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIQAShimKeyForQaOnly0000000000000000000 overseer-qa', fingerprint: 'SHA256:qaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQaQ', installCommand: "echo 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIQAShimKeyForQaOnly0000000000000000000 overseer-qa' >> ~/.ssh/authorized_keys" }),
    'plugin:event|listen': () => nextId++,
    'plugin:event|unlisten': () => null,
    term_detach: (a) => { terms.get(a.attach)?.close(); terms.delete(a.attach); },
  };
  const NULL_COMMANDS = new Set([
    'conn_answer', 'conn_wake', 'conn_restart_server',
    'ports_forward', 'ports_unforward',
    'browser_open', 'browser_bounds', 'browser_navigate', 'browser_back', 'browser_forward', 'browser_reload', 'browser_hide', 'browser_show', 'browser_close', 'browser_pages_reset',
    'update_install', 'update_relaunch',
    'open_local_network_settings', 'open_notification_settings', 'start_dictation',
    'direct_host_save', 'direct_host_delete', 'direct_host_forget_key',
    'diag_log', 'diag_visibility', 'diag_session_end', 'diag_dismiss', 'diag_save', 'diag_open_dir',
    'apk_update_status', 'apk_update_check', 'apk_update_download', 'apk_update_install', 'apk_update_allow_install',
    'mobile_minimize', 'mobile_clipboard_image',
  ]);
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback: (f, once) => { const id = nextCb++; cbs.set(id, (x) => { if (once) cbs.delete(id); f(x); }); return id; },
    unregisterCallback: (id) => cbs.delete(id),
    convertFileSrc: (p) => p,
    invoke: async (cmd, args = {}, opts) => {
      const h = handlers[cmd];
      if (h) return h(args, opts);
      log('unhandled', cmd);
      if (cmd.startsWith('plugin:') || NULL_COMMANDS.has(cmd)) return null;
      const err = new Error('qa shim: unknown Tauri command ' + cmd);
      console.error(err.message);
      throw err;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
})();
