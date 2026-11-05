(() => {
  const PORT = '__QA_PORT__';
  const BASE = 'http://127.0.0.1:' + PORT, WS = 'ws://127.0.0.1:' + PORT;
  const cbs = new Map(); let nextCb = 1, nextId = 1;
  const subs = new Map(), terms = new Map();
  const idx = new Map();
  const send = (ch, message) => { if (!ch) return; const id = ch.id; const i = idx.get(id) ?? 0; idx.set(id, i + 1); const f = cbs.get(id); if (f) f({ index: i, message }); };
  window.__qaLog = [];
  const log = (...a) => window.__qaLog.push(a.map(String).join(' '));
  let up = Promise.resolve(), wake = null, watch = null;
  async function alive() {
    try { const r = await fetch(BASE + '/v1/version', { signal: AbortSignal.timeout(5000) }); return r.ok ? ((await r.json()).version ?? '') : null; } catch { return null; }
  }
  function monitor(ch, target) {
    const me = { stop: false }; watch = me;
    (async () => {
      for (;;) {
        await new Promise((r) => setTimeout(r, 5000));
        if (me.stop) return;
        if ((await alive()) !== null) continue;
        up = new Promise((r) => (wake = r));
        for (let attempt = 0; !me.stop; attempt++) {
          send(ch, { phase: 'reconnecting', target, attempt, detail: 'the daemon stopped answering' });
          await new Promise((r) => setTimeout(r, 1000));
          const version = await alive();
          if (version !== null) { send(ch, { phase: 'connected', target, version }); break; }
        }
        wake?.(); wake = null;
      }
    })();
  }
  async function req(method, path, body) {
    await up;
    const r = await fetch(BASE + path, { method, headers: body == null ? {} : { 'Content-Type': 'application/json' }, body: body == null ? undefined : JSON.stringify(body) });
    const t = await r.text(); let b = null; try { b = t ? JSON.parse(t) : null; } catch { b = t; }
    return { status: r.status, body: b };
  }
  function events(ch) {
    const id = nextId++; const ac = new AbortController(); subs.set(id, ac);
    let last = null;
    (async () => {
      while (!ac.signal.aborted) {
        try {
          await up; const r = await fetch(BASE + '/v1/events', { headers: last ? { Accept: 'text/event-stream', 'Last-Event-ID': last } : { Accept: 'text/event-stream' }, signal: ac.signal });
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
  function attach(termId, ch) {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(WS + '/v1/terms/' + encodeURIComponent(termId) + '/io'); ws.binaryType = 'arraybuffer';
      const id = nextId++; terms.set(id, ws);
      ws.onopen = () => resolve(id);
      ws.onerror = (e) => { log('ws err', termId); reject('terminal attach failed'); };
      ws.onmessage = (m) => { if (m.data instanceof ArrayBuffer) send(ch, Array.from(new Uint8Array(m.data))); };
      ws.onclose = () => send(ch, []);
    });
  }
  const handlers = {
    daemon_request: (a) => req(a.method, a.path, a.body),
    daemon_bytes: async (a) => { const r = await fetch(BASE + a.path); if (!r.ok) { const t = await r.text(); let e = `HTTP ${r.status}`; try { e = JSON.parse(t).error || e; } catch {} throw { status: r.status, error: e }; } send(a.onData, Array.from(new Uint8Array(await r.arrayBuffer()))); },
    daemon_events: (a) => events(a.onEvent),
    daemon_events_stop: (a) => { subs.get(a.sub)?.abort(); subs.delete(a.sub); },
    term_attach: (a) => attach(a.termId, a.onData),
    term_write: (a) => { terms.get(a.attach)?.send(new TextEncoder().encode(a.data)); },
    term_resize: (a) => { terms.get(a.attach)?.send(JSON.stringify({ resize: { cols: a.cols, rows: a.rows }, client: 'desktop' })); },
    term_background: () => null,
    diag_tail: () => ({ path: 'app.log', lines: ['qa line'] }),
    diag_info: () => null,
    ports_watch: (a) => { const id = nextId++; const tick = async () => { try { const r = await fetch(BASE + '/v1/ports'); send(a.onState, { remote: false, detected: await r.json(), forwards: [] }); } catch {} }; tick(); subs.set(id, { abort: () => clearInterval(iv) }); const iv = setInterval(tick, 1500); return id; },
    ports_watch_stop: (a) => { subs.get(a.sub)?.abort(); subs.delete(a.sub); },
    ports_configure: () => null,
    app_platform: () => { const os = new URLSearchParams(location.search).get('qaos'); return os ? { os, mobile: os === 'android' || os === 'ios' } : { os: 'linux', mobile: false }; },
    'plugin:clipboard-manager|write_text': (a) => { window.__qaClip = a.text; return null; },
    'plugin:clipboard-manager|read_text': () => window.__qaClip ?? '',
    save_download: (a, o) => { window.__qaDownload = { name: decodeURIComponent(o?.headers?.['x-name'] ?? ''), size: a?.byteLength ?? a?.length ?? 0 }; return '/home/qa/Downloads/' + window.__qaDownload.name; },
    conn_connect: async (a) => { if (watch) watch.stop = true; send(a.onState, { phase: 'connected', target: a.target, version: (await alive()) ?? '0.2.0' }); monitor(a.onState, a.target); },
    conn_disconnect: () => { if (watch) watch.stop = true; watch = null; wake?.(); wake = null; up = Promise.resolve(); return null; },
    ssh_hosts: () => [],
    direct_hosts: () => [],
    'plugin:event|listen': () => nextId++,
    'plugin:event|unlisten': () => null,
    term_detach: (a) => { terms.get(a.attach)?.close(); terms.delete(a.attach); },
  };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback: (f, once) => { const id = nextCb++; cbs.set(id, (x) => { if (once) cbs.delete(id); f(x); }); return id; },
    unregisterCallback: (id) => cbs.delete(id),
    convertFileSrc: (p) => p,
    invoke: async (cmd, args = {}, opts) => {
      const h = handlers[cmd];
      if (!h) { log('unhandled', cmd); if (cmd.startsWith('plugin:')) return null; return null; }
      return h(args, opts);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
})();
