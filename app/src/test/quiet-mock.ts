import { MockDaemon } from '../lib/daemon/mock';

export const quietMock = (options: NonNullable<ConstructorParameters<typeof MockDaemon>[0]> = {}) => new MockDaemon({ live: false, delayMs: 0, ...options });
