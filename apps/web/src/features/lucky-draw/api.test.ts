import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { LIST_KEY, drawKey, putDraw } from "./api";
import { D1, detail } from "./fixtures";
import type { LuckyDrawDetail } from "./types";

describe("putDraw", () => {
  it("keeps a later version already in the cache (a spin's answer held while the wheel turned)", () => {
    const client = new QueryClient();
    putDraw(client, detail({ version: 5, name: "sau" }));
    putDraw(client, detail({ version: 4, name: "trước" }));
    expect(client.getQueryData<LuckyDrawDetail>(drawKey(D1))?.name).toBe("sau");
    putDraw(client, detail({ version: 6, name: "mới nhất" }));
    expect(client.getQueryData<LuckyDrawDetail>(drawKey(D1))?.name).toBe("mới nhất");
  });

  it("marks the list's counts stale", () => {
    const client = new QueryClient();
    client.setQueryData(LIST_KEY, []);
    putDraw(client, detail());
    expect(client.getQueryState(LIST_KEY)?.isInvalidated).toBe(true);
  });
});
