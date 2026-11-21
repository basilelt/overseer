export const ACTIVITY_MS = 400;
export const ACTIVITY_BACKGROUND_MS = 2500;

export function activityDelay(selected: boolean, visible: boolean): number {
  return selected && visible ? ACTIVITY_MS : ACTIVITY_BACKGROUND_MS;
}

const HTML_TAG =
  /<\/?(?:a|b|i|u|s|p|br|hr|em|strong|code|pre|span|div|kbd|sub|sup|small|mark|del|ins|li|ul|ol|h[1-6]|img|blockquote)\b[^>]*>/gi;
const CODE_OR_TEXT = /(`+)([\s\S]*?)(?<!`)\1(?!`)|([^`]+)|`+/g;

function stripTags(text: string): string {
  let prev: string;
  do {
    prev = text;
    text = text.replace(HTML_TAG, '');
  } while (text !== prev);
  return text;
}

function unmark(text: string): string {
  return stripTags(text)
    .replace(/!?\[([^\]\n]*)\]\([^)\s]*\)/g, '$1')
    .replace(/(\*\*|__)/g, '');
}

export function plainSnippet(text: string): string {
  return text
    .replace(/^#+ /gm, '')
    .replace(CODE_OR_TEXT, (_m, fence: string | undefined, code: string | undefined, rest: string | undefined) =>
      fence ? (code ?? '') : unmark(rest ?? ''),
    );
}
