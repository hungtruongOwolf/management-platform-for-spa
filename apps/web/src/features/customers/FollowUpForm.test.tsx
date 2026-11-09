import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
// The appointments chunk words `too_far` as its 90-day booking rule: loaded or not, this form keeps its own.
import { APPOINTMENT_FIELDS } from "../appointments/api";
import { FollowUpForm } from "./FollowUpForm";

describe("FollowUpForm server errors", () => {
  it("words the next-try date's own codes and ties the text to the date field", async () => {
    expect(APPOINTMENT_FIELDS["too_far"]).toBe("Chỉ đặt lịch trước tối đa 90 ngày.");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      Response.json(
        { status: 422, code: "validation_failed", errors: [{ field: "next_follow_up_on", code: "too_far" }] },
        { status: 422 },
      ),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <FollowUpForm customerId="c1" customerName="Nguyễn Thị Hà" onDone={() => undefined} onCancel={() => undefined} />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole("button", { name: "Lưu lần liên hệ" }));
    const message = await screen.findByText("Ngày hẹn lại tối đa 365 ngày sau ngày liên hệ.");
    expect(screen.queryByText("Chỉ đặt lịch trước tối đa 90 ngày.")).not.toBeInTheDocument();
    // A screen reader hears the refusal on the field itself (aria-describedby), not only nearby.
    const field = screen.getByLabelText("Hẹn liên hệ lại");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(field).toHaveAccessibleDescription(expect.stringContaining("Ngày hẹn lại tối đa 365 ngày sau ngày liên hệ."));
    expect(field.getAttribute("aria-describedby")?.split(" ")).toContain(message.id);
  });
});
