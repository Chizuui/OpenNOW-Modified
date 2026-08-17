import React from "react";
import ReactDOM from "react-dom/client";
import { scan } from "react-scan";

import { initLogCapture } from "@shared/logger";
import { App } from "./App";
import { MotionProvider } from "./components/MotionProvider";
import { initializeLocale } from "./i18n";
import "./styles.css";

// Initialize log capture for renderer process
initLogCapture("renderer");
void initializeLocale();

// React Scan instruments every React render and is useful during UI work, but
// it adds measurable renderer/compositor pressure while a WebRTC stream is
// being captured by Discord or the window is being occluded. Keep it opt-in so
// development streaming has the same lightweight render path as production.
if (import.meta.env.DEV && import.meta.env.VITE_ENABLE_REACT_SCAN === "1") {
  scan();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MotionProvider>
      <App />
    </MotionProvider>
  </React.StrictMode>,
);
