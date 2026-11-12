let fmt: Intl.DateTimeFormat | null = null;

export function hhmm(at: string): string {
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return '';
  fmt ??= new Intl.DateTimeFormat([], { hour: '2-digit', minute: '2-digit' });
  return fmt.format(d);
}
