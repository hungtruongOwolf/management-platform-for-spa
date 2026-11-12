import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";

// The error texts (lib/errors.ts) are not in the startup chunk: the sign-in page fetches them after
// its first paint. Here they never load, as when the network is down.
vi.mock("../../lib/errors", () => {
  throw new Error("chunk not reachable");
});

const screens = vi.hoisted(() => ({ preloaded: [] as string[] }));
vi.mock("../../app/screens", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../app/screens")>();
  return {
    ...actual,
    preloadScreen: (href: string) => {
      screens.preloaded.push(href);
    },
  };
});

function signInAt(path: string, login: () => Promise<Response>) {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.pathname : input.url;
    if (url === "/api/v1/auth/login") return login();
    return Promise.resolve(new Response(JSON.stringify({ status: 401, code: "auth.unauthenticated" }), { status: 401 }));
  });
  render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
}

async function submit() {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("Số điện thoại"), "0905123456");
  await user.type(screen.getByLabelText("Mật khẩu"), "mat-khau");
  await user.click(screen.getByRole("button", { name: "Đăng nhập" }));
}

describe("sign-in without the error texts chunk", () => {
  it("still says the network is down when even the texts cannot be fetched", async () => {
    signInAt("/dang-nhap", () => Promise.reject(new TypeError("Failed to fetch")));
    await submit();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Không kết nối được máy chủ. Vui lòng kiểm tra mạng.",
    );
    expect(screen.getByLabelText("Mật khẩu")).toHaveValue("");
  });

  it("starts the chunk of the screen sign-in leads to together with the request", async () => {
    screens.preloaded.length = 0;
    signInAt("/dang-nhap?next=%2Fquay%3Fview%3Dlich", () => new Promise<Response>(() => {}));
    await submit();
    expect(screens.preloaded).toEqual(["/quay?view=lich"]);
  });
});
