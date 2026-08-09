import { useEffect, useState } from "react";
import type { NativeStreamerStatus } from "@shared/gfn";

/**
 * The native streamer status is cached in the main process
 * (`probeStatus()` reuses the child's hello capabilities while it is alive),
 * so this fetch is cheap and safe to repeat. The renderer-side promise is
 * cached for the whole session so every consumer shares one probe result.
 */
let nativeStreamerStatusPromise: Promise<NativeStreamerStatus | null> | null = null;

function fetchNativeStreamerStatus(): Promise<NativeStreamerStatus | null> {
  nativeStreamerStatusPromise ??= (async () => {
    try {
      if (!window.openNow?.getNativeStreamerStatus) {
        return null;
      }
      return await window.openNow.getNativeStreamerStatus();
    } catch {
      return null;
    }
  })();
  return nativeStreamerStatusPromise;
}

export interface NativeStreamerStatusState {
  status: NativeStreamerStatus | null;
  loading: boolean;
}

/**
 * Subscribe to the native streamer capability status. When `enabled` is false
 * (web mode) the state stays empty; when enabled, the status is fetched once
 * per session (shared cache) and delivered asynchronously.
 */
export function useNativeStreamerStatus(enabled: boolean): NativeStreamerStatusState {
  const [state, setState] = useState<NativeStreamerStatusState>({
    status: null,
    loading: enabled,
  });

  useEffect(() => {
    if (!enabled) {
      setState({ status: null, loading: false });
      return;
    }
    let cancelled = false;
    setState((current) => ({
      status: current.status,
      loading: current.status === null,
    }));
    void fetchNativeStreamerStatus().then((status) => {
      if (!cancelled) {
        setState({ status, loading: false });
      }
    });
    return () => {
      cancelled = true;
    };
  }, [enabled]);

  return state;
}
