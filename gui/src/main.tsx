import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-sans/700.css";
import "@fontsource/ibm-plex-mono/400.css";
import App from "./App";
import { ErrorBoundary } from "./components/troubleshooting/ErrorBoundary";
import { RenderErrorProbe } from "./components/troubleshooting/RenderErrorProbe";
import { reportUncaught } from "./lib/errorReport";
import "./index.css";

reportUncaught();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <RenderErrorProbe />
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
