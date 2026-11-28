import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useGrowing } from "./useGrowing";

const ITEMS = Array.from({ length: 400 }, (_, i) => `Khách ${String(i + 1)}`);

function List({ items, resetKey }: { items: readonly string[]; resetKey: string }) {
  const { shown, more, sentinel } = useGrowing(items, resetKey);
  return (
    <div className="ld-scroll">
      <ul aria-label="Danh sách">
        {shown.map((n) => (
          <li key={n}>{n}</li>
        ))}
        {more && <li key={`more-${String(shown.length)}`} ref={sentinel} data-testid="more" aria-hidden="true" />}
      </ul>
    </div>
  );
}

/** A stand-in IntersectionObserver: `seeEnd()` tells the current observer its target came in view. */
function stubObserver() {
  const observers: { cb: IntersectionObserverCallback; root: Element | Document | null | undefined; target?: Element | undefined }[] = [];
  vi.stubGlobal(
    "IntersectionObserver",
    class {
      private readonly entry: (typeof observers)[number];
      constructor(cb: IntersectionObserverCallback, opts?: IntersectionObserverInit) {
        this.entry = { cb, root: opts?.root };
        observers.push(this.entry);
      }
      observe(target: Element) {
        this.entry.target = target;
      }
      disconnect() {
        this.entry.target = undefined;
      }
    },
  );
  return {
    observers,
    seeEnd: () => {
      const live = observers.filter((o) => o.target);
      for (const o of live) o.cb([{ isIntersecting: true, target: o.target } as IntersectionObserverEntry], {} as IntersectionObserver);
    },
  };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("useGrowing", () => {
  it("draws 150 rows, 150 more each time the end comes near, then drops the marker", () => {
    const io = stubObserver();
    render(<List items={ITEMS} resetKey="all|" />);
    const list = screen.getByRole("list", { name: "Danh sách" });
    expect(list.querySelectorAll("li:not([data-testid])")).toHaveLength(150);
    expect(io.observers.at(-1)?.root).toHaveClass("ld-scroll");
    act(io.seeEnd);
    expect(list.querySelectorAll("li:not([data-testid])")).toHaveLength(300);
    act(io.seeEnd);
    expect(list.querySelectorAll("li:not([data-testid])")).toHaveLength(400);
    expect(screen.queryByTestId("more")).not.toBeInTheDocument();
  });

  it("starts again from the first step when the filter changes", () => {
    const io = stubObserver();
    const { rerender } = render(<List items={ITEMS} resetKey="all|" />);
    act(io.seeEnd);
    expect(screen.getByRole("list").querySelectorAll("li:not([data-testid])")).toHaveLength(300);
    rerender(<List items={ITEMS.filter((_, i) => i % 2 === 0)} resetKey="female|" />);
    expect(screen.getByRole("list").querySelectorAll("li:not([data-testid])")).toHaveLength(150);
  });

  it("draws everything where IntersectionObserver does not exist", () => {
    vi.stubGlobal("IntersectionObserver", undefined);
    render(<List items={ITEMS} resetKey="all|" />);
    expect(screen.getByRole("list").querySelectorAll("li")).toHaveLength(400);
  });
});
