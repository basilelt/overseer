<script lang="ts" module>
  const GLYPHS: Record<string, string> = {
    claude: 'M8 2.5v11M3.24 5.25l9.52 5.5M3.24 10.75l9.52-5.5',
    codex: 'M5.5 4.5 2.5 8l3 3.5M10.5 4.5l3 3.5-3 3.5M9 3.5 7 12.5',
    gemini: 'M8 2.5c.5 3 2.5 5 5.5 5.5-3 .5-5 2.5-5.5 5.5-.5-3-2.5-5-5.5-5.5 3-.5 5-2.5 5.5-5.5Z',
    agy: 'M8 12V3M4.5 6.5 8 3l3.5 3.5M3.5 13.5h9',
    opencode: 'M6 3H3.5v10H6M10 3h2.5v10H10',
    amp: 'M9 2.5 4 9h4l-1 4.5 5-6.5H8l1-4.5Z',
    copilot: 'M2.5 8.5a2.5 2.5 0 1 0 5 0 2.5 2.5 0 1 0-5 0M8.5 8.5a2.5 2.5 0 1 0 5 0 2.5 2.5 0 1 0-5 0M7.5 8.5h1M2.5 8.5V6.5c0-1.7 2.5-3 5.5-3s5.5 1.3 5.5 3v2',
    cursor: 'M3.5 2.5 12.5 7 8.5 8.2 6.7 12.5Z',
    shell: 'M3 4.5 6 8l-3 3.5M8 11.5h5',
  };
  const UNKNOWN = 'M2.5 8a5.5 5.5 0 1 0 11 0 5.5 5.5 0 1 0-11 0M6.8 8a1.2 1.2 0 1 0 2.4 0 1.2 1.2 0 1 0-2.4 0';

  export function glyphPath(agent: string): string {
    return GLYPHS[agent] ?? UNKNOWN;
  }
</script>

<script lang="ts">
  let { agent, size = 16 }: { agent: string; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 16 16"
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  data-agent={agent}
>
  <path d={glyphPath(agent)} />
</svg>
