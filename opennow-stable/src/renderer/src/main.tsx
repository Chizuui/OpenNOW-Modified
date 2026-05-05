import React from "react";
import ReactDOM from "react-dom/client";
import { scan } from "react-scan";

import { initLogCapture } from "@shared/logger";
import { App } from "./App";
import { initializeLocale } from "./i18n";
import { installWebOsRuntime } from "./platform/webos/runtime";
import "./styles.css";

// Initialize log capture for renderer process
initLogCapture("renderer");
void initializeLocale();
installWebOsRuntime();
void initializeLocale();
installWebOsRuntime();

if (import.meta.env.DEV) {
  scan();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
