import { GROUP_STATE, type GroupId } from './grouping';
import type { SessionState } from './session-state';

export const stateOfGroup = (group: GroupId): SessionState => GROUP_STATE[group];
