import { describeExit, iso, type DiagInfo, type PreviousSession, type ServerLog } from './diag';

export interface BundleInput {
  generatedAt: number;
  info: DiagInfo | null;
  previous: PreviousSession | null;
  appLog: string[];
  server: ServerLog | null;
  host?: string;
}

function section(title: string, body: string[]): string {
  return [`===== ${title} =====`, ...(body.length ? body : ['(empty)']), ''].join('\n');
}

export function formatBundle(b: BundleInput): string {
  const head = ['Overseer diagnostic bundle', `Generated: ${iso(b.generatedAt)}`];
  if (b.info) {
    head.push(`App: ${b.info.version} (server ${b.info.serverVersion}), ${b.info.os} ${b.info.arch}`);
    if (b.info.logDir) head.push(`Log folder: ${b.info.logDir}`);
  }
  if (b.host) head.push(`Host: ${b.host}`);
  const parts = [head.join('\n') + '\n'];
  if (b.previous) {
    const p = b.previous;
    parts.push(
      section('Previous session', [
        `Started: ${iso(p.startedAt)}`,
        `Last log line: ${p.endedAt ?? '?'}`,
        `Version: ${p.version || '?'}`,
        `Outcome: ${describeExit(p)}`,
      ]),
    );
    if (p.crashReport) parts.push(section('Crash report', p.crashReport.trimEnd().split('\n')));
    parts.push(section(`App log before the exit (last ${p.appLog.length} lines)`, p.appLog));
  } else {
    parts.push(section(`App log (last ${b.appLog.length} lines)`, b.appLog));
  }
  parts.push(section('Server log', serverBody(b.server)));
  return parts.join('\n').trimEnd() + '\n';
}

function serverBody(s: ServerLog | null): string[] {
  if (!s || s.status === 'offline') return ['(not connected to a server)'];
  if (s.status === 'unsupported') return ['(the server is too old to send its log; it is in ~/.overseer/daemon.log on the host)'];
  if (s.status === 'error') return [`(could not fetch the server log: ${s.message})`];
  return [`# ${s.tail.path} (last ${s.tail.lines.length} lines)`, ...s.tail.lines];
}
