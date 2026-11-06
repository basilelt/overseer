import type { GroupId } from './grouping';
import type { SessionState } from './session-state';

// Test items carry the process state their group implies; only the bucket groups need a choice.
export const stateOfGroup = (group: GroupId): SessionState =>
  group === 'pushed' || group === 'needs_testing' ? 'ready' : group;
