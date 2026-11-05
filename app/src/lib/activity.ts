export const ACTIVITY_MS = 400;
export const ACTIVITY_BACKGROUND_MS = 2500;

export function activityDelay(selected: boolean, visible: boolean): number {
  return selected && visible ? ACTIVITY_MS : ACTIVITY_BACKGROUND_MS;
}
