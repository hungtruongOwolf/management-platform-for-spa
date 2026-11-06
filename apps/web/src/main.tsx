// CI scope demo (web-only change); this PR is closed without merging.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./app/App";
import { applyTheme, readTheme } from "./app/prefs";

// Before the first render, so a device set to dark never flashes the light theme.
applyTheme(readTheme());

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
