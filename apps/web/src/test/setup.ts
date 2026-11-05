import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup, configure } from "@testing-library/react";

// Most screens are lazy route chunks: the first `findBy*` of a file waits for a dynamic import,
// which on a busy machine (parallel workers, slow disk) can take more than the 1 s default.
configure({ asyncUtilTimeout: 4000 });

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
