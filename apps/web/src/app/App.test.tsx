import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";

function mockFetch(response: Response | Error) {
  return vi.spyOn(globalThis, "fetch").mockImplementation(() =>
    response instanceof Error ? Promise.reject(response) : Promise.resolve(response),
  );
}

describe("App shell", () => {
  it("shows the brand and a connected API status with its version", async () => {
    const fetchSpy = mockFetch(Response.json({ status: "ok", version: "0.1.0" }));
    render(<App />);

    expect(screen.getByRole("heading", { name: "Viên Phúc Y Đường" })).toBeInTheDocument();
    expect(await screen.findByText("API: đã kết nối (v0.1.0)")).toBeInTheDocument();
    expect(fetchSpy).toHaveBeenCalledWith("/api/v1/health", expect.objectContaining({ credentials: "same-origin" }));
  });

  it("shows a Vietnamese error when the API is unreachable", async () => {
    mockFetch(new TypeError("Failed to fetch"));
    render(<App />);
    expect(await screen.findByText("API: không kết nối được")).toBeInTheDocument();
  });

  it("treats a non-OK response as disconnected", async () => {
    mockFetch(new Response("boom", { status: 503 }));
    render(<App />);
    expect(await screen.findByText("API: không kết nối được")).toBeInTheDocument();
  });
});
