import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./app/App";
import { applyTheme, readTheme } from "./app/prefs";

// Before the first render, so a device set to dark never flashes the light theme.
applyTheme(readTheme());

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing from index.html");

// The platform console (ADR-0018) is a separate app in its own chunk: the spa's startup bundle
// only carries this branch.
if (/^\/admin(\/|$)/.test(location.pathname)) {
  void import("./admin/boot").then((m) => {
    m.mountAdmin(root);
  });
} else {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
