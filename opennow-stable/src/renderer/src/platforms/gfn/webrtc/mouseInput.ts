/** Coalesce pointer samples like official GFN wm() when bursts are large. */
export function subsampleCoalescedPointerEvents<T extends { movementX: number; movementY: number }>(
  samples: readonly T[],
  pendingBatchEntries: number,
  maxBatchEntries: number = 16,
): { events: T[]; stride: number } {
  if (samples.length <= 1) {
    return { events: [...samples], stride: 1 };
  }

  const budget = samples.length > 2 * maxBatchEntries
    ? 1
    : Math.max(maxBatchEntries - pendingBatchEntries - 4, 1);
  if (samples.length <= budget) {
    return { events: [...samples], stride: 1 };
  }

  const stride = Math.ceil(samples.length / budget);
  const events: T[] = [];
  for (let index = 0; index < samples.length; index += stride) {
    const end = Math.min(index + stride, samples.length);
    let movementX = 0;
    let movementY = 0;
    for (let sampleIndex = index; sampleIndex < end; sampleIndex += 1) {
      movementX += samples[sampleIndex]!.movementX;
      movementY += samples[sampleIndex]!.movementY;
    }
    events.push({
      ...samples[end - 1]!,
      movementX,
      movementY,
    } as T);
  }
  return { events, stride };
}

export function quantizeMouseDeltaWithResidual(accumulatedDelta: number): { send: number; residual: number } {
  const send = Math.round(accumulatedDelta);
  return {
    send,
    residual: accumulatedDelta - send,
  };
}

export interface RelativeMouseDelta {
  dxServer: number;
  dyServer: number;
  residualX: number;
  residualY: number;
}

/**
 * Convert an accumulated floating-point mouse delta (already sensitivity- and
 * acceleration-adjusted by the caller) into an integer server delta with the
 * fractional remainder carried as residual, clamped to the wire format's i16
 * range. Deliberately NO server-width ÷ window-width normalization: raw-input
 * games calibrate their sensitivity on raw counts, so window-size scaling
 * would make the feel depend on the window size and break muscle memory
 * (local play has no such scaling either). Returns null when there is nothing
 * to send (both axes below half a count, or both clamp to zero).
 */
export function computeRelativeMouseDelta(
  accumulatedDx: number,
  accumulatedDy: number,
): RelativeMouseDelta | null {
  if (Math.abs(accumulatedDx) < 0.5 && Math.abs(accumulatedDy) < 0.5) {
    return null;
  }
  const dxQuantized = quantizeMouseDeltaWithResidual(accumulatedDx);
  const dyQuantized = quantizeMouseDeltaWithResidual(accumulatedDy);
  const dxServer = Math.max(-32768, Math.min(32767, dxQuantized.send));
  const dyServer = Math.max(-32768, Math.min(32767, dyQuantized.send));
  if (dxServer === 0 && dyServer === 0) {
    return null;
  }
  return {
    dxServer,
    dyServer,
    residualX: dxQuantized.residual,
    residualY: dyQuantized.residual,
  };
}

/** Filters noisy/outlier relative mouse deltas before they enter the send path. */
export class MouseDeltaFilter {
  private x = 0;
  private y = 0;
  private lastTsMs = 0;
  private velocityX = 0;
  private velocityY = 0;
  private rejectedX = 0;
  private rejectedY = 0;
  private pendingX = 0;
  private pendingY = 0;
  private sawZero = false;
  private relaxedForRawInput = false;

  public setRelaxedForRawInput(value: boolean): void {
    this.relaxedForRawInput = value;
  }

  public getX(): number {
    return this.x;
  }

  public getY(): number {
    return this.y;
  }

  public reset(): void {
    this.x = 0;
    this.y = 0;
    this.lastTsMs = 0;
    this.velocityX = 0;
    this.velocityY = 0;
    this.rejectedX = 0;
    this.rejectedY = 0;
    this.pendingX = 0;
    this.pendingY = 0;
    this.sawZero = false;
  }

  public update(dx: number, dy: number, tsMs: number): boolean {
    if (dx === 0 && dy === 0) {
      return false;
    }
    this.x = dx;
    this.y = dy;
    this.lastTsMs = tsMs;
    return true;
  }
}
