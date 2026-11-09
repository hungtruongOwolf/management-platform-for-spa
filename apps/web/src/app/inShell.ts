import { createContext } from "react";

/**
 * True below the shell's `<main>`: a status page rendered there is a section, not a second
 * `<main id="main">` (one main landmark per page). Its own module so the lazy StatusPage chunk
 * stays out of the startup bundle.
 */
export const InShell = createContext(false);
