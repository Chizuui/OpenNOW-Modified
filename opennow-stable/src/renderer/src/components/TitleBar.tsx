import { useEffect, useState, type JSX } from "react";
import { Copy, Minus, Square, X } from "lucide-react";
import { OpenNowLogoMark } from "./OpenNowLogoMark";

/**
 * Custom frameless window title bar (GFN-style chrome).
 *
 * Windows/Linux use fully custom window controls (minimize / maximize /
 * close). macOS keeps the native traffic lights via `titleBarStyle: "hidden"`
 * and only renders the brand + drag region.
 *
 * Double-click on the drag region maximizes/restores natively (the drag
 * region is treated as a non-client area by the OS, so no JS handler needed).
 *
 * While a stream is active the brand fades out for an immersive look; the
 * window controls stay available (closing/minimizing a session still matters).
 */
export function TitleBar({
  streaming = false,
  fullscreen = false,
}: {
  streaming?: boolean;
  fullscreen?: boolean;
}): JSX.Element {
  const [maximized, setMaximized] = useState(false);
  const isMac =
    navigator.userAgent.includes("Macintosh") ||
    navigator.platform?.toLowerCase().includes("mac");

  useEffect(() => {
    let mounted = true;
    void window.openNow.getMaximizeWindowState().then((state) => {
      if (mounted) {
        setMaximized(state);
      }
    });
    const unsubscribe = window.openNow.onMaximizeWindowStateChanged(setMaximized);
    return () => {
      mounted = false;
      unsubscribe();
    };
  }, []);

  const handleToggleMaximize = (): void => {
    void window.openNow.toggleMaximizeWindow().then(setMaximized);
  };

  return (
    <div className={`titlebar${isMac ? " titlebar--darwin" : ""}${streaming ? " titlebar--streaming" : ""}${fullscreen ? " titlebar--fullscreen" : ""}`}>
      <div className="titlebar-brand">
        <OpenNowLogoMark className="titlebar-logo" />
        <span className="titlebar-title">OpenNOW</span>
      </div>
      {!isMac && (
        <div className="titlebar-controls">
          <button
            type="button"
            className="titlebar-btn"
            onClick={() => window.openNow.minimizeWindow()}
            aria-label="Minimize"
            title="Minimize"
          >
            <Minus size={14} />
          </button>
          <button
            type="button"
            className="titlebar-btn"
            onClick={handleToggleMaximize}
            aria-label={maximized ? "Restore" : "Maximize"}
            title={maximized ? "Restore" : "Maximize"}
          >
            {maximized ? <Copy size={12} /> : <Square size={12} />}
          </button>
          <button
            type="button"
            className="titlebar-btn titlebar-close"
            onClick={() => window.openNow.closeWindow()}
            aria-label="Close"
            title="Close"
          >
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
