import asyncio, mimetypes, os, sys
SOCK, PORT = sys.argv[1], int(sys.argv[2])
DIST = sys.argv[3] if len(sys.argv) > 3 else None


async def pipe(r, w):
    try:
        while (b := await r.read(65536)):
            w.write(b); await w.drain()
    except Exception: pass
    finally:
        try: w.close()
        except Exception: pass


async def serve_static(path, cw):
    rel = path.split('?', 1)[0].lstrip('/') or 'index.html'
    file = os.path.realpath(os.path.join(DIST, rel))
    if not file.startswith(os.path.realpath(DIST) + os.sep) or not os.path.isfile(file):
        file = os.path.join(DIST, 'index.html')
    body = open(file, 'rb').read()
    kind = mimetypes.guess_type(file)[0] or 'application/octet-stream'
    cw.write(f'HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {len(body)}\r\nConnection: close\r\n\r\n'.encode() + body)
    await cw.drain(); cw.close()


async def handle(cr, cw):
    try:
        head = await cr.readuntil(b'\r\n\r\n')
    except Exception:
        cw.close(); return
    path = head.split(b' ', 2)[1].decode('latin-1') if head.count(b' ') >= 2 else '/'
    if DIST and not path.startswith('/v1/'):
        await serve_static(path, cw); return
    if b'upgrade:' not in head.lower():
        lines = [l for l in head[:-4].split(b'\r\n') if not l.lower().startswith(b'connection:')]
        head = b'\r\n'.join(lines + [b'Connection: close']) + b'\r\n\r\n'
    try: ur, uw = await asyncio.open_unix_connection(SOCK)
    except Exception: cw.close(); return
    uw.write(head); await uw.drain()
    await asyncio.gather(pipe(cr, uw), pipe(ur, cw))


async def main():
    s = await asyncio.start_server(handle, '127.0.0.1', PORT)
    async with s: await s.serve_forever()
asyncio.run(main())
