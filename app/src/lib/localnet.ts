import type { LocalNetworkReport } from './types';

export type LocalNetworkView = 'none' | 'permission' | 'unreachable';
export type DiagnosticId = 'endpoint' | 'via' | 'addresses' | 'interface' | 'connect' | 'multicast' | 'signing' | 'bundle' | 'error';

export function isNoRoute(error: string | null | undefined): boolean {
  return !!error && /no route to host|EHOSTUNREACH|os error 65\b/i.test(error);
}

export function localNetworkView(report: LocalNetworkReport | null | undefined, error: string | null | undefined): LocalNetworkView {
  if (!report || report.status === 'notApplicable') return 'none';
  if (report.status === 'denied') return 'permission';
  if (!isNoRoute(error)) return 'none';
  return report.status === 'unknown' ? 'permission' : 'unreachable';
}

export function unstableIdentity(report: LocalNetworkReport | null | undefined): boolean {
  return report?.signing === 'adhoc' || report?.signing === 'unsigned';
}

export const MAIN_BINARY = 'overseer-app';

export function terminalCommand(report: LocalNetworkReport | null | undefined): string | null {
  const bundle = report?.bundle;
  if (!bundle) return null;
  const exe = /\.app\/?$/.test(bundle) ? `${bundle.replace(/\/$/, '')}/Contents/MacOS/${MAIN_BINARY}` : bundle;
  return `'${exe.replace(/'/g, `'\\''`)}'`;
}

export function diagnostics(report: LocalNetworkReport): [DiagnosticId, string][] {
  const rows: [DiagnosticId, string | null | undefined][] = [
    ['endpoint', report.endpoint],
    ['via', report.via],
    ['addresses', report.addresses.join(', ')],
    ['interface', report.interface],
    ['connect', report.connect],
    ['multicast', report.multicast],
    ['signing', report.signing],
    ['bundle', report.bundle],
    ['error', report.error],
  ];
  return rows.filter((r): r is [DiagnosticId, string] => !!r[1]);
}
