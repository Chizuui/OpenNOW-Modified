/**
 * Merges a muxer's (data, position) writes into a contiguous append-only byte
 * stream. Both mp4-muxer and webm-muxer emit merged, ascending sections
 * (overlapping rewrites — moof patches, CodecPrivate placeholders — are
 * collapsed inside each flush), so the only non-contiguity is the occasional
 * gap, buffered here until it fills. flushAll() drains whatever remains after
 * finalize().
 */
export class PositionalWriter {
  private readonly chunks = new Map<number, Uint8Array>();
  private nextEmit = 0;

  constructor(private readonly onData: (data: Uint8Array) => void) {}

  write(data: Uint8Array, position: number): void {
    if (position < this.nextEmit) {
      // Backward write into already-emitted bytes. Should not happen after the
      // muxer's own merge; dropping keeps the append-only file consistent.
      return;
    }
    this.chunks.set(position, data);
    this.flush();
  }

  flushAll(): void {
    this.flush();
  }

  private flush(): void {
    let data = this.chunks.get(this.nextEmit);
    while (data) {
      this.chunks.delete(this.nextEmit);
      this.nextEmit += data.byteLength;
      this.onData(data);
      data = this.chunks.get(this.nextEmit);
    }
  }
}
