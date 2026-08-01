export interface AdaptiveMouseFlushDecisionParams {
  baseIntervalMs: number;
  currentIntervalMs: number;
  reliableBufferedAmount: number;
  schedulingDelayMs: number;
  canUsePartiallyReliableMouse: boolean;
  backpressureThresholdBytes: number;
  minIntervalMs: number;
  maxIntervalMs: number;
}

export function chooseAdaptiveMouseFlushInterval(params: AdaptiveMouseFlushDecisionParams): number {
  const boundedBase = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.baseIntervalMs));
  const boundedCurrent = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.currentIntervalMs));
  // Official GFN keeps a fixed coalesce interval (4/8/16 ms) for PR mouse and does not
  // back off because the reliable keyboard channel is busy.
  if (params.canUsePartiallyReliableMouse) {
    return boundedBase;
  }

  const highPressure =
    params.reliableBufferedAmount >= params.backpressureThresholdBytes / 2
    || params.schedulingDelayMs >= 4;
  if (highPressure) {
    return Math.max(boundedBase, Math.min(params.maxIntervalMs, boundedCurrent + 2));
  }

  const lowPressure = params.reliableBufferedAmount <= 4096 && params.schedulingDelayMs <= 1;
  if (lowPressure) {
    return Math.max(params.minIntervalMs, boundedCurrent - 1);
  }

  if (boundedCurrent > boundedBase) {
    return Math.max(boundedBase, boundedCurrent - 1);
  }
  if (boundedCurrent < boundedBase) {
    return Math.min(boundedBase, boundedCurrent + 1);
  }
  return boundedCurrent;
}

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
