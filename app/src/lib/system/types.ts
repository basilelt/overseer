export interface SysPoint {
  t: number;
  cpu: number | null;
  memUsed: number;
  memTotal: number;
  swapUsed: number;
  rx: number;
  tx: number;
  read: number;
  write: number;
}

export interface SysProc {
  pid: number;
  name: string;
  cpu: number;
  rss: number;
}

export interface SysAgent {
  termId: string;
  lineId: string | null;
  agent: string | null;
  title: string;
  cwd: string;
  pid: number;
  procs: number;
  cpu: number;
  rss: number;
}

export interface SysFs {
  mount: string;
  device: string;
  fstype: string;
  total: number;
  used: number;
  avail: number;
}

export interface SysSample {
  t: number;
  platform: string;
  supported: boolean;
  uptimeSec?: number | null;
  load?: [number, number, number] | null;
  cpu?: { total: number | null; cores: number[] };
  pressure?: { cpu: number | null; memory: number | null; io: number | null } | null;
  mem?: { total: number; used: number; available: number; cached: number; swapTotal: number; swapUsed: number };
  net?: { name: string; rx: number; tx: number }[];
  disks?: { name: string; read: number; write: number }[];
  fs?: SysFs[];
  top?: { cpu: SysProc[]; mem: SysProc[] };
  agents?: SysAgent[];
}

export interface SysResponse {
  intervalMs: number;
  windowMs: number;
  running: boolean;
  current: SysSample | null;
  history: SysPoint[];
}
