import { GROUP_STATE, type GroupId } from '../lib/session/grouping';
import type { SessionState } from '../lib/session/session-state';

export const stateOfGroup = (group: GroupId): SessionState => GROUP_STATE[group];
