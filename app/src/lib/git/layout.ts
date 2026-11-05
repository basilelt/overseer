export interface Seg {
  kind: 'pass' | 'in' | 'out';
  from: number;
  to: number;
  color: number;
}

export interface Row {
  lane: number;
  color: number;
  segs: Seg[];
  width: number;
}

export interface LayoutState {
  lanes: (string | null)[];
  colors: number[];
  next: number;
}

export const EMPTY_STATE: LayoutState = { lanes: [], colors: [], next: 0 };

export function layout(commits: readonly { sha: string; parents: readonly string[] }[], prev: LayoutState = EMPTY_STATE): { rows: Row[]; state: LayoutState } {
  const lanes = prev.lanes.slice();
  const colors = prev.colors.slice();
  let next = prev.next;
  const rows: Row[] = new Array(commits.length);
  const free = () => {
    const i = lanes.indexOf(null);
    return i < 0 ? lanes.length : i;
  };

  for (let r = 0; r < commits.length; r++) {
    const c = commits[r];
    const segs: Seg[] = [];
    let node = -1;
    let width = lanes.length;
    for (let k = 0; k < lanes.length; k++) {
      if (lanes[k] === c.sha) {
        if (node < 0) node = k;
        segs.push({ kind: 'in', from: k, to: -1, color: colors[k] });
      }
    }
    let color: number;
    if (node < 0) {
      node = free();
      color = next++;
    } else {
      color = colors[node];
    }
    for (const s of segs) s.to = node;
    for (let k = 0; k < lanes.length; k++) {
      if (lanes[k] === c.sha) lanes[k] = null;
      else if (lanes[k] !== null) segs.push({ kind: 'pass', from: k, to: k, color: colors[k] });
    }
    for (let i = 0; i < c.parents.length; i++) {
      const p = c.parents[i];
      const at = lanes.indexOf(p);
      if (at >= 0) {
        segs.push({ kind: 'out', from: node, to: at, color: colors[at] });
        continue;
      }
      const lane = i === 0 && lanes[node] == null ? node : free();
      lanes[lane] = p;
      colors[lane] = i === 0 ? color : next++;
      segs.push({ kind: 'out', from: node, to: lane, color: colors[lane] });
    }
    while (lanes.length && lanes[lanes.length - 1] === null) lanes.pop();
    colors.length = lanes.length;
    width = Math.max(width, node + 1, lanes.length);
    rows[r] = { lane: node, color, segs, width };
  }
  return { rows, state: { lanes, colors, next } };
}

export function maxWidth(rows: readonly Row[], start = 0, end = rows.length): number {
  let w = 1;
  for (let i = start; i < end; i++) if (rows[i].width > w) w = rows[i].width;
  return w;
}
