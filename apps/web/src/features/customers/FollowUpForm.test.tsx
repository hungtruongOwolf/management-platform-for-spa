import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { addMessages } from "../../lib/errors";
import { FollowUpForm } from "./FollowUpForm";

describe("FollowUpForm server errors", () => {
  it("words the next-try date's own codes, whatever another screen registered for the same code", async () => {
    // The appointments chunk maps `too_far` to its 90-day booking rule.
    addMessages({}, { too_far: "Chỉ đặt lịch trước tối đa 90 ngày." });
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
    expect(await screen.findByText("Ngày hẹn lại tối đa 365 ngày sau ngày liên hệ.")).toBeInTheDocument();
    expect(screen.queryByText("Chỉ đặt lịch trước tối đa 90 ngày.")).not.toBeInTheDocument();
  });
});
