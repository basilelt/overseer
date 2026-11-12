import { GROUP_STATE, type GroupId } from '../lib/grouping';
import type { SessionState } from '../lib/session-state';

export const stateOfGroup = (group: GroupId): SessionState => GROUP_STATE[group];
