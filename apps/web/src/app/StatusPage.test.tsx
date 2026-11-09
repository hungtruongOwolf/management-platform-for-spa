import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { InShell } from "./inShell";
import { ErrorPage } from "./StatusPage";

describe("ErrorPage", () => {
  it("is the page's main landmark on its own", () => {
    render(<ErrorPage error={new Error("boom")} />);
    expect(screen.getByRole("main")).toHaveAttribute("id", "main");
  });

  it("inside the shell it adds no second main (the shell's <main id=main> stays the only one)", () => {
    render(
      <main id="main">
        <InShell value>
          <ErrorPage error={new Error("boom")} />
        </InShell>
      </main>,
    );
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(document.querySelectorAll("#main")).toHaveLength(1);
    expect(screen.getByRole("heading", { level: 1, name: "Màn hình này gặp lỗi" })).toBeInTheDocument();
  });
});
