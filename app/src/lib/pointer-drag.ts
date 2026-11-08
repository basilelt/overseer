type PointerCaptureTarget = Pick<Element, 'hasPointerCapture' | 'releasePointerCapture'>;

/** Release pointer capture when a gesture ends through blur as well as pointerup/cancel. */
export function releaseCapturedPointer(target: PointerCaptureTarget | null | undefined, pointerId: number | null): boolean {
  if (!target || pointerId === null) return false;
  try {
    if (!target.hasPointerCapture(pointerId)) return false;
    target.releasePointerCapture(pointerId);
    return true;
  } catch {
    // The browser may already have released capture as it cancels a pointer gesture.
    return false;
  }
}
