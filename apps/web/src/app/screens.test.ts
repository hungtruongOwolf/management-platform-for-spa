import { describe, expect, it, vi } from "vitest";
import { preloadScreen } from "./screens";

const imported = vi.hoisted(() => [] as string[]);
vi.mock("../features/front-desk/FrontDeskPage", () => {
  imported.push("/quay");
  return { FrontDeskPage: () => null };
});
vi.mock("./HomePage", () => {
  imported.push("/");
  return { HomePage: () => null, prefetchHome: () => undefined };
});

describe("preloadScreen", () => {
  it("starts the chunk of the screen a path opens, whatever its query; other paths start nothing", async () => {
    preloadScreen("/khach-hang?q=ha");
    preloadScreen("/quay?view=lich");
    await vi.waitFor(() => {
      expect(imported).toEqual(["/quay"]);
    });
    preloadScreen("/#top");
    await vi.waitFor(() => {
      expect(imported).toEqual(["/quay", "/"]);
    });
  });
});
