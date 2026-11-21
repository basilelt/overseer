export const ACTIVITY_MS = 400;
export const ACTIVITY_BACKGROUND_MS = 2500;

export function activityDelay(selected: boolean, visible: boolean): number {
  return selected && visible ? ACTIVITY_MS : ACTIVITY_BACKGROUND_MS;
}

export function plainSnippet(text: string): string {
  return text
    .replace(/<\/?(?:a|b|i|u|s|p|br|hr|em|strong|code|pre|span|div|kbd|sub|sup|small|mark|del|ins|li|ul|ol|h[1-6]|img|blockquote)\b[^>]*>/gi, '')
    .replace(/^#+ /gm, '')
    .replace(/!?\[([^\]\n]*)\]\([^)\s]*\)/g, '$1')
    .replace(/(\*\*|__|`)/g, '');
}
