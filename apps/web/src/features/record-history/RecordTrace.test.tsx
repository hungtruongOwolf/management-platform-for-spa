import type { ActivityEntry, AuditAction, RecordHistory } from "@vpyd/api-client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { createQueryClient } from "../../lib/queryClient";
import { HISTORY, moneyOf } from "../activity/sentence";
import { visitChip } from "./chip";
import { canTrace, recordHistoryQuery } from "./query";
import { RecordTrace } from "./RecordTrace";
import { changeLines, metaText } from "./text";

const VISIT = "0193a5b0-0000-7000-8000-0000000000e1";
const HA = { kind: "user" as const, user_id: "0193a5b0-0000-7000-8000-0000000000d1", name: "Hà" };
const LAN = { kind: "user" as const, user_id: "0193a5b0-0000-7000-8000-0000000000d2", name: "Lan" };
/** 12/11/2026 in the spa's zone (UTC+7): 03:07Z is 10:07 there. */
const DAY = "2026-11-12";

const line = (e: Partial<ActivityEntry> & Pick<ActivityEntry, "action">): ActivityEntry => ({
  id: crypto.randomUUID(),
  kind: "front_desk",
  at: `${DAY}T03:07:00Z`,
  day: DAY,
  actor: HA,
  changes: [],
  packages: [],
  folded: [],
  ...e,
});

const CREATED = line({ action: "visit.create", amount: 300_000, folded: ["sale.create"] });
const KTV = line({
  action: "visit.update",
  actor: LAN,
  at: `${DAY}T03:30:00Z`,
  changes: [{ field: "staff", format: "text", old: "Thu", new: "Hoa" }],
});

const history = (over: Partial<RecordHistory> = {}): RecordHistory => ({
  record: { type: "visit", id: VISIT },
  summary: { created: { actor: HA, at: `${DAY}T03:07:00Z`, day: DAY, source: "audit" }, edits: 1, state: "active" },
  items: [CREATED, KTV],
  omitted: 0,
  ...over,
});

/** Answers every request with `reply` (a body, or a status with a problem code). */
function serve(...replies: (RecordHistory | number)[]) {
  const spy = vi.spyOn(globalThis, "fetch");
  for (const reply of replies) {
    spy.mockResolvedValueOnce(
      typeof reply === "number"
        ? Response.json({ status: reply, code: reply === 403 ? "auth.forbidden" : reply === 404 ? "not_found" : "error" }, { status: reply })
        : Response.json(reply),
    );
  }
  return spy;
}

function renderTrace(props: Partial<Parameters<typeof RecordTrace>[0]> = {}, client: QueryClient = createQueryClient()) {
  render(
    <QueryClientProvider client={client}>
      <h3 id="title" tabIndex={-1}>
        Lượt 10:05
      </h3>
      <RecordTrace record={{ type: "visit", id: VISIT }} noun="lượt 10:05 của chị Mai" day={DAY} {...props} />
    </QueryClientProvider>,
  );
  return client;
}

describe("RecordTrace", () => {
  it("reads the meta line from the summary, closed, then opens and closes the history in place", async () => {
    const fetch = serve(history());
    renderTrace();
    const button = await screen.findByRole("button", { name: "Ghi bởi Hà · 10:07 · đã sửa 1 lần" });
    expect(fetch).toHaveBeenCalledWith(`/api/v1/record-history/visit/${VISIT}`, expect.anything());
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("list", { name: "Lịch sử lượt 10:05 của chị Mai" })).toBeNull();

    await userEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveFocus();
    const list = screen.getByRole("list", { name: "Lịch sử lượt 10:05 của chị Mai" });
    expect(button.getAttribute("aria-controls")).toBe(list.parentElement?.id);
    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("10:07Hà ghi lượt · Thu 300.000");
    expect(items[1]).toHaveTextContent("10:30Lan sửa lượtKTV: Thu → Hoa");
    expect(items[0]?.querySelector("time")).toHaveAttribute("datetime", CREATED.at);

    await userEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("list", { name: /^Lịch sử/ })).toBeNull();
  });

  it("says it is loading on a stable button until the reply is in", async () => {
    let answer: (r: Response) => void = () => undefined;
    vi.spyOn(globalThis, "fetch").mockReturnValue(
      new Promise((resolve) => {
        answer = resolve;
      }),
    );
    renderTrace();
    const button = screen.getByRole("button", { name: "Đang tải ai ghi, ai sửa…" });
    expect(button).toHaveAttribute("aria-busy", "true");
    act(() => {
      answer(Response.json(history()));
    });
    await waitFor(() => {
      expect(button).toHaveAccessibleName("Ghi bởi Hà · 10:07 · đã sửa 1 lần");
    });
    expect(button).not.toHaveAttribute("aria-busy");
  });

  it("opens at once when asked (the ledger's sheet), and dates a line of another day", async () => {
    serve(
      history({
        items: [CREATED, line({ action: "visit.void", actor: LAN, at: "2026-11-13T02:00:00Z", day: "2026-11-13", amount: 300_000, reason: "khách đổi ý" })],
        summary: { created: { actor: HA, at: `${DAY}T03:07:00Z`, day: DAY, source: "audit" }, edits: 0, state: "voided" },
      }),
    );
    renderTrace({ defaultOpen: true });
    expect(await screen.findByRole("button", { name: "Ghi bởi Hà · 10:07 · đã hủy" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByRole("listitem")[1]).toHaveTextContent("09:00 13/11Lan hủy lượt · Trả lại 300.000đ · lý do: khách đổi ý");
  });

  it("network or server trouble: says so and tries again on request", async () => {
    const fetch = serve(500, history());
    renderTrace();
    expect(await screen.findByText(/^Không tải được lịch sử\./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Thử lại" }));
    expect(await screen.findByRole("button", { name: /^Ghi bởi Hà/ })).toBeInTheDocument();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("404: the record cannot be shown, with nothing to retry", async () => {
    serve(404);
    renderTrace();
    expect(await screen.findByText("Không xem được lịch sử của mục này.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Thử lại" })).toBeNull();
  });

  it("403 (the switch went off mid-session): the block hides, focus goes to the record's title, /me is asked again", async () => {
    serve(history(), 403);
    const forbidden = vi.fn();
    const client = renderTrace({ focusOnHide: () => document.getElementById("title"), onForbidden: forbidden });
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const button = await screen.findByRole("button", { name: /^Ghi bởi Hà/ });
    button.focus();
    // A write elsewhere invalidates every open history (lib/queryClient.ts): the next read is refused.
    await act(() => client.invalidateQueries({ queryKey: ["record-history"] }));
    await waitFor(() => {
      expect(screen.queryByRole("button", { name: /^Ghi bởi/ })).toBeNull();
    });
    expect(document.querySelector(".record-trace")).toBeNull();
    expect(screen.getByRole("heading", { name: "Lượt 10:05" })).toHaveFocus();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["me"] });
    expect(forbidden).toHaveBeenCalledTimes(1);
  });

  it("403 on the first read leaves focus where the person is, and tells the screen", async () => {
    serve(403);
    const forbidden = vi.fn();
    const elsewhere = document.createElement("button");
    document.body.append(elsewhere);
    elsewhere.focus();
    renderTrace({ focusOnHide: () => document.getElementById("title"), onForbidden: forbidden });
    await waitFor(() => {
      expect(forbidden).toHaveBeenCalledTimes(1);
    });
    expect(elsewhere).toHaveFocus();
    expect(document.querySelector(".record-trace")).toBeNull();
    elsewhere.remove();
  });

  it("empty and cut histories say so", async () => {
    serve(history({ items: [], summary: { edits: 0, state: "active" } }));
    renderTrace({ defaultOpen: true });
    expect(await screen.findByRole("button", { name: "Chưa rõ ai ghi" })).toBeInTheDocument();
    expect(screen.getByText("Chưa có lịch sử thay đổi.")).toBeInTheDocument();
  });

  it("names a former member and the platform team the way the activity screen does", async () => {
    serve(
      history({
        summary: { created: { actor: { ...HA, left: true }, at: `${DAY}T03:07:00Z`, day: DAY, source: "audit" }, edits: 0, state: "active" },
        items: [
          line({ action: "visit.create", actor: { ...HA, left: true } }),
          line({ action: "visit.update", actor: { kind: "platform" }, changes: [{ field: "check_in_at", format: "instant" }] }),
        ],
        omitted: 3,
      }),
    );
    renderTrace({ defaultOpen: true });
    expect(await screen.findByRole("button", { name: "Ghi bởi Hà (đã nghỉ) · 10:07" })).toBeInTheDocument();
    expect(screen.getByText("Còn 3 thay đổi cũ hơn.")).toBeInTheDocument();
    expect(screen.getByText("Hà (đã nghỉ) ghi lượt")).toBeInTheDocument();
    expect(screen.getByText("Đội vận hành sửa lượt")).toBeInTheDocument();
  });

  it("external mode: the screen's control opened it, so the meta line and the list show as they are", async () => {
    serve(
      history({
        record: { type: "customer_package", id: VISIT },
        summary: { created: { actor: HA, at: `${DAY}T03:07:00Z`, day: DAY, source: "audit" }, edits: 0, state: "cancelled" },
        items: [
          line({ action: "customer_package.create", kind: "packages", packages: ["Gội 10 buổi"], amount: 1_500_000 }),
          line({ action: "customer_package.cancel", kind: "packages", actor: LAN, at: "2026-12-20T02:00:00Z", day: "2026-12-20", amount: 1_500_000, reason: "chuyển nhà" }),
        ],
      }),
    );
    renderTrace({ mode: "external", id: "pkg-trace", record: { type: "customer_package", id: VISIT }, noun: "gói Gội 10 buổi", day: undefined });
    expect(await screen.findByText("Ghi bởi Hà · 12/11/2026 · đã hủy")).toBeInTheDocument();
    expect(document.getElementById("pkg-trace")).toHaveAttribute("data-row-inert");
    expect(screen.queryByRole("button")).toBeNull();
    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("10:07Hà bán gói Gội 10 buổi · Giá 1.500.000");
    expect(items[1]).toHaveTextContent("09:00 20/12Lan hủy gói · Hoàn 1.500.000đ · lý do: chuyển nhà");
  });

  it("shows a phone change only as the server sends it: values to the owner, the bare field to others", async () => {
    serve(
      history({
        record: { type: "customer", id: VISIT },
        summary: { created: { actor: HA, at: "2025-02-03T02:00:00Z", day: "2025-02-03", source: "audit" }, edits: 2, state: "active" },
        items: [
          line({ action: "customer.update", kind: "customers", changes: [{ field: "phone", format: "phone", old: "+84905111222", new: "+84912333444" }] }),
          line({ action: "customer.update", kind: "customers", changes: [{ field: "phone", format: "phone" }] }),
        ],
      }),
    );
    renderTrace({ record: { type: "customer", id: VISIT }, noun: "hồ sơ Lê Thị Mai", day: undefined, defaultOpen: true });
    expect(await screen.findByRole("button", { name: "Tạo bởi Hà · 03/02/2025 · đã sửa 2 lần" })).toBeInTheDocument();
    expect(screen.getByText("SĐT: 0905 111 222 → 0912 333 444")).toBeInTheDocument();
    expect(screen.getByText("SĐT (đã đổi)")).toBeInTheDocument();
  });

  it("the phone card it sits in stays open: a click inside is no click on the card", async () => {
    serve(history());
    // A phone: the row is a card, its details inside the card that a click folds.
    vi.stubGlobal(
      "matchMedia",
      (query: string) =>
        ({ matches: query.includes("max-width"), media: query, addEventListener: () => undefined, removeEventListener: () => undefined }) as unknown as MediaQueryList,
    );
    const onRowClick = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ResponsiveTable
          caption="Lượt"
          rows={[{ id: "v1" }]}
          rowKey={(r) => r.id}
          columns={[{ id: "a", header: "A", cell: () => "Chị Mai" }]}
          onRowClick={onRowClick}
          expanded={() => <RecordTrace record={{ type: "visit", id: VISIT }} noun="lượt" defaultOpen />}
        />
      </QueryClientProvider>,
    );
    // The words of a line and its time are no control, yet reading them never folds the card.
    await userEvent.click(await screen.findByText("Hà ghi lượt · Thu 300.000đ"));
    await userEvent.click(screen.getByText("KTV: Thu → Hoa"));
    await userEvent.click(screen.getAllByText("10:07")[0] as HTMLElement);
    expect(onRowClick).not.toHaveBeenCalled();
    await userEvent.click(screen.getByText("Chị Mai"));
    expect(onRowClick).toHaveBeenCalledTimes(1);
    vi.unstubAllGlobals();
  });
});

describe("metaText", () => {
  const base = history();
  it("a backdated visit says the day it was recorded", () => {
    expect(metaText({ ...base, summary: { ...base.summary, created: { actor: HA, at: "2026-11-12T08:10:00Z", day: DAY, source: "audit" } } }, "2026-11-10")).toBe(
      "Ghi bởi Hà · 15:10 12/11 · đã sửa 1 lần",
    );
  });
  it("another year adds it", () => {
    expect(metaText(base, "2027-01-02")).toBe("Ghi bởi Hà · 10:07 12/11/2026 · đã sửa 1 lần");
  });
  it("a paper-book package names the book's day, never who extended it", () => {
    expect(
      metaText({
        ...base,
        record: { type: "customer_package", id: VISIT },
        summary: { created: { day: "2025-02-03", source: "paper_book" }, edits: 0, state: "active" },
      }),
    ).toBe("Nhập từ sổ giấy · 03/02/2025");
  });
  it("a package never says 'đã sửa' (its lifecycle is no edit)", () => {
    expect(metaText({ ...base, record: { type: "customer_package", id: VISIT }, summary: { ...base.summary, edits: 3 } })).toBe("Ghi bởi Hà · 12/11/2026");
  });
  // Wave 2: a booking, a staff-book cell, a bonus / penalty say the time they were written, and
  // the date when that was another day than the record's own.
  it("a booking made days before its day dates the making; a cancelled one says so", () => {
    const booking = { ...base, record: { type: "appointment" as const, id: VISIT } };
    expect(metaText(booking, "2026-11-14")).toBe("Ghi bởi Hà · 10:07 12/11 · đã sửa 1 lần");
    expect(metaText({ ...booking, summary: { ...booking.summary, edits: 0, state: "cancelled" } }, DAY)).toBe("Ghi bởi Hà · 10:07 · đã hủy");
  });
  it("a staff-book cell: its writes after the first are edits; nobody ever wrote it → 'Chưa có ai ghi'", () => {
    const cell = { ...base, record: { type: "attendance" as const } };
    expect(metaText({ ...cell, summary: { ...cell.summary, edits: 2 } }, "2026-11-11")).toBe("Ghi bởi Hà · 10:07 12/11 · đã sửa 2 lần");
    const empty = { record: { type: "daily_review" as const }, summary: { edits: 0, state: "active" as const }, items: [], omitted: 0 };
    expect(metaText(empty, DAY)).toBe("Chưa có ai ghi");
    // A record that exists but has no creating row stays "Chưa rõ ai ghi".
    expect(metaText({ ...empty, record: { type: "customer", id: VISIT } })).toBe("Chưa rõ ai ghi");
  });
  it("a bonus edited once: 'đã sửa 1 lần'", () => {
    expect(metaText({ ...base, record: { type: "adjustment", id: VISIT } }, DAY)).toBe("Ghi bởi Hà · 10:07 · đã sửa 1 lần");
  });
});

describe("recordHistoryQuery / canTrace (wave 2)", () => {
  it("a staff-book cell is asked by person, day and book; the others by id", async () => {
    const fetch = serve(history({ record: { type: "attendance" } }), history({ record: { type: "adjustment", id: VISIT } }));
    const client = createQueryClient();
    await client.fetchQuery(recordHistoryQuery({ type: "attendance", staffId: "s1", date: DAY }));
    await client.fetchQuery(recordHistoryQuery({ type: "adjustment", id: VISIT }));
    expect(fetch.mock.calls.map(([url]) => url)).toEqual([`/api/v1/record-history/staff-day/s1/${DAY}?book=attendance`, `/api/v1/record-history/adjustment/${VISIT}`]);
    expect(recordHistoryQuery({ type: "daily_review", staffId: "s1", date: DAY }).queryKey).toEqual(["record-history", "daily_review", "s1", DAY]);
  });
  it("each record's own screen right, after the history right", () => {
    const me = (permissions: string[]) => ({ permissions }) as unknown as Parameters<typeof canTrace>[0];
    const all = ["record.history.read", "appointment.manage", "attendance.record", "evaluation.record"];
    for (const anchor of ["appointment", "attendance", "daily_review", "adjustment"] as const) {
      expect(canTrace(me(all), anchor)).toBe(true);
      expect(canTrace(me(all.slice(1)), anchor)).toBe(false);
    }
    expect(canTrace(me(["record.history.read", "attendance.record"]), "daily_review")).toBe(false);
    expect(canTrace(me(["record.history.read", "evaluation.record"]), "attendance")).toBe(false);
    expect(canTrace(me(["record.history.read", "evaluation.record"]), "appointment")).toBe(false);
  });
});

describe("HISTORY", () => {
  // Spec record-history §4.2: shown another way already, or not about a record the spa opens.
  const NONE: AuditAction[] = [
    "customer.follow_up",
    "evaluation.record",
    "staff.create",
    "staff.update",
    "staff.end",
    "staff.rehire",
    "staff.pay_update",
    "staff.account_link",
    "payroll.close",
    "staff.login_create",
    "staff.login_password_reset",
    "staff.login_role",
    "account.password_change",
    "account.password_reset_requested",
    "account.password_reset_dismissed",
    "account.locked_by_platform",
    "account.unlocked_by_platform",
    "account.mfa_reset_by_platform",
    "account.sessions_revoked_by_platform",
    "account.password_reset_by_platform",
    "role.permissions_changed",
    "role.permissions_reset",
    "feature.changed",
    "tenant.created",
    "tenant.suspended",
    "tenant.unsuspended",
    "tenant.owner_mfa_changed",
  ];

  it("places every audit action (a Record over AuditAction: a new one fails the typecheck): 37 with words, 27 without", () => {
    const keys = Object.keys(HISTORY) as AuditAction[];
    expect(keys).toHaveLength(64);
    expect(keys.filter((k) => HISTORY[k] === null).sort()).toEqual([...NONE].sort());
    expect(keys.filter((k) => HISTORY[k] !== null)).toHaveLength(37);
  });

  it("words by context", () => {
    const say = (e: Partial<ActivityEntry> & Pick<ActivityEntry, "action">) => HISTORY[e.action as AuditAction]?.(line(e));
    expect(say({ action: "visit.create", detail: "backdated" })).toBe("nhập bù lượt");
    expect(say({ action: "visit.create", packages: ["Gội 10 buổi"] })).toBe("ghi lượt kèm bán gói Gội 10 buổi");
    expect(say({ action: "visit.addition" })).toBe("thêm vào lượt");
    expect(say({ action: "visit.convert_to_package", packages: ["Gội 10 buổi"] })).toBe("chuyển lượt sang gói Gội 10 buổi");
    expect(say({ action: "sale.create" })).toBe("thu tiền");
    expect(say({ action: "staff.adjustment", detail: "penalty" })).toBe("ghi phạt");
    expect(say({ action: "attendance.record", detail: "cleared" })).toBe("xóa chấm công");
    // An edit of a bonus / penalty: the void folded into the new entry (K21).
    expect(say({ action: "staff.adjustment", detail: "bonus", folded: ["staff.adjustment_void"] })).toBe("sửa khoản thưởng");
    expect(say({ action: "appointment.update" })).toBe("đổi lịch");
  });

  it("the changes of wave 2 say what changed, never a note's text", () => {
    const lines = (e: ActivityEntry) => changeLines(e);
    expect(
      lines(
        line({
          action: "staff.adjustment",
          changes: [
            { field: "person", format: "text", old: "KTV Lan", new: "KTV Mai" },
            { field: "amount", format: "money", old: "200000", new: "300000" },
            { field: "kind", format: "code", old: "bonus", new: "penalty" },
          ],
        }),
      ),
    ).toEqual(["Nhân viên: KTV Lan → KTV Mai", "Số tiền: 200.000đ → 300.000đ", "Loại: Thưởng → Phạt"]);
    expect(
      lines(
        line({
          action: "appointment.update",
          changes: [
            { field: "services", format: "text" },
            { field: "booking_note", format: "text" },
          ],
        }),
      ),
    ).toEqual(["Dịch vụ (đã đổi)", "Ghi chú lịch hẹn (đã đổi)"]);
    expect(lines(line({ action: "attendance.record", changes: [{ field: "has_note", format: "flag" }] }))).toEqual(["Ghi chú (đã đổi)"]);
  });

  it("a cancel gives money back: 'Trả lại', not 'Đảo'", () => {
    expect(moneyOf(line({ action: "visit.void", amount: 140_000 }))).toMatch(/^Trả lại 140\.000/);
    expect(moneyOf(line({ action: "sale.void", amount: 140_000 }))).toMatch(/^Trả lại 140\.000/);
  });
});

describe("visitChip (spec D9: one chip, exclusive)", () => {
  it.each([
    [{ corrected_by_visit_id: "b", voided: true, edited: true }, "Đã thay"],
    [{ corrected_by_visit_id: "b", voided: true }, "Đã thay"],
    [{ voided: true, edited: true }, "Đã hủy"],
    [{ voided: true }, "Đã hủy"],
    [{ voided: false, edited: true }, "Đã sửa"],
    [{ voided: false, edited: false }, null],
    // Without the history right the server sends no `edited`: no "Đã sửa".
    [{ voided: false }, null],
  ])("%j → %s", (marks, label) => {
    expect(visitChip(marks)?.label ?? null).toBe(label);
  });
});
