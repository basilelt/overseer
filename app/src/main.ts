import './lib/demo-boot';
import '@fontsource/inter/latin-400.css';
import '@fontsource/inter/latin-500.css';
import '@fontsource/inter/latin-600.css';
import '@fontsource/jetbrains-mono/latin-400.css';
import '@fontsource/jetbrains-mono/latin-700.css';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';
import { applyTheme, settings } from './lib/settings.svelte';
import { loadLocale } from './lib/i18n.svelte';

applyTheme();

const target = document.getElementById('app')!;

export default import.meta.env.VITE_PERF === '1'
  ? import('./lib/perf/Harness.svelte').then((m) => mount(m.default, { target }))
  : loadLocale(settings.locale).then(() => mount(App, { target }));
