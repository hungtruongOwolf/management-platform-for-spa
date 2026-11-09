import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SaleNoteForm } from "./SaleNoteForm";

describe("SaleNoteForm", () => {
  it("ties the server's refusal of the note to the field (aria-describedby), after its hint", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      Response.json({ status: 422, code: "validation_failed", errors: [{ field: "note", code: "too_long" }] }, { status: 422 }),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <SaleNoteForm saleId="s1" note="Đau vai gáy" customerId="c1" title="Ghi chú gói" onDone={() => undefined} onCancel={() => undefined} />
      </QueryClientProvider>,
    );
    const field = screen.getByRole("textbox", { name: "Ghi chú gói" });
    expect(field).toHaveAccessibleDescription(expect.stringMatching(/^Bảo mật như ghi chú buổi/));
    await userEvent.click(screen.getByRole("button", { name: /Lưu/ }));
    const message = await screen.findByText("Quá dài.");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(field.getAttribute("aria-describedby")?.split(" ")).toContain(message.id);
    expect(field).toHaveAccessibleDescription(expect.stringContaining("Quá dài."));
  });
});
