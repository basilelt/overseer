import { core } from './parts/core.ts';
import { settingsEn } from './parts/settings.ts';
import { tourEn } from './parts/tour.ts';

export const en = { ...core, ...settingsEn, ...tourEn };
