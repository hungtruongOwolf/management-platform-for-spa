import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { loadFontsAfterFirstScreen } from "../app/fonts";
import { AdminApp } from "./AdminApp";
import "./messages";

/** Entry of the platform console (`/admin/*`), loaded by main.tsx as its own chunk. */
export function mountAdmin(root: HTMLElement): void {
  createRoot(root).render(
    <StrictMode>
      <AdminApp />
    </StrictMode>,
  );
  loadFontsAfterFirstScreen();
}
