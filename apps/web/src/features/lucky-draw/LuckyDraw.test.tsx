import { createMemoryHistory } from "@tanstack/react-router";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../../app/App";
import { navFor } from "../../app/nav";
import { D1, detail, person, prize, result } from "./fixtures";
import { OWNER, fakeApi } from "./testServer";

const at = (path: string) => render(<App history={createMemoryHistory({ initialEntries: [path] })} />);
const MANAGER = { ...OWNER, permissions: ["customer.read", "visit.read", "report.read"] };

/** The wheel's turning disk on a stage. */
function diskOf(stage: HTMLElement): HTMLElement {
  const disk = stage.querySelector<HTMLElement>(".ld-wheel-disk");
  if (!disk) throw new Error("no wheel disk");
  return disk;
}

/** The table row holding `el`. */
function rowOf(el: HTMLElement): HTMLElement {
  const row = el.closest("tr");
  if (!row) throw new Error("not in a table row");
  return row;
}

/** Two women, two men; "Cổ vai gáy" for women only (1 slot), "Gội đầu" for anyone (2 slots). */
const DRAW = detail({
  participants: [person("p1", "Nguyễn Thị Lan", "female", 0), person("p2", "Trần Văn Minh", "male", 1), person("p3", "Lê Thu Hà", "female", 2), person("p4", "Phạm Bảo", "male", 3)],
  prizes: [
    prize("q1", "Cổ vai gáy", { eligible: "female", quantity: 1, remaining: 1, eligible_count: 2, position: 0 }),
    prize("q2", "Gội đầu", { quantity: 2, remaining: 2, eligible_count: 4, position: 1 }),
  ],
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("who sees the lucky draw (spec §2)", () => {
  it("is in the Khách hàng group only with lucky_draw.manage", () => {
    const items = (me: typeof OWNER) => navFor(me).find((g) => g.label === "Khách hàng")?.items.map((i) => i.label);
    expect(items(OWNER)).toEqual(["Danh sách khách hàng", "Khách lâu chưa quay lại", "Vòng quay may mắn"]);
    expect(items(MANAGER)).not.toContain("Vòng quay may mắn");
  });

  it("a manager opening /vong-quay gets “Không có quyền” and no call to the API", async () => {
    const api = fakeApi(MANAGER);
    at("/vong-quay");
    expect(await screen.findByRole("heading", { name: "Không có quyền mở màn hình này" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Vòng quay may mắn" })).not.toBeInTheDocument();
    expect(api.calls.some((c) => c.path.startsWith("/api/v1/lucky-draws"))).toBe(false);
  });
});

describe("list (US1)", () => {
  it("teaches the first step when there is no program, then creates one and opens it", async () => {
    const api = fakeApi();
    const user = userEvent.setup();
    at("/vong-quay");
    expect(await screen.findByText("Chưa có chương trình nào")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tạo chương trình" }));
    const sheet = await screen.findByRole("dialog", { name: "Tạo chương trình" });
    await user.click(within(sheet).getByRole("button", { name: "Tạo chương trình" }));
    expect(within(sheet).getByText("Nhập tên chương trình.")).toBeInTheDocument();
    await user.type(within(sheet).getByLabelText("Tên chương trình"), "  Tri ân 20/11 ");
    await user.click(within(sheet).getByRole("button", { name: "Tạo chương trình" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tri ân 20/11" })).toBeInTheDocument();
    expect(api.calls.find((c) => c.method === "POST")?.body).toEqual({ name: "Tri ân 20/11" });
    // The new program needs no second request: its creation answered with it.
    expect(api.calls.filter((c) => c.method === "GET" && /lucky-draws\/[0-9a-f-]{36}$/.test(c.path))).toHaveLength(0);
    expect(screen.getByText(/Thêm người tham gia và giải thưởng để bắt đầu quay/)).toBeInTheDocument();
  });

  it("lists each program with its counts and status", async () => {
    fakeApi(OWNER, [detail({ ...DRAW, results: [result("r1", 1, "q2", "p1", false)] })]);
    at("/vong-quay");
    const row = rowOf(await screen.findByRole("link", { name: "Tri ân 20/11" }));
    expect(within(row).getByText("4")).toBeInTheDocument();
    expect(within(row).getByText("1/3")).toBeInTheDocument();
    expect(within(row).getByText("Đang quay")).toBeInTheDocument();
  });
});

describe("spin (US4)", () => {
  it("starts on the first prize that can be drawn, and says why the others cannot", async () => {
    fakeApi(OWNER, [
      detail({
        ...DRAW,
        prizes: [prize("q0", "Hết rồi", { quantity: 1, remaining: 0, awarded: 1, position: 0 }), ...DRAW.prizes.map((p) => ({ ...p, position: p.position + 1 }))],
        results: [result("r0", 1, "q0", "p4")],
        participants: DRAW.participants.map((p) => (p.id === "p4" ? { ...p, result_id: "r0" } : p)),
      }),
    ]);
    at(`/vong-quay/${D1}`);
    const list = await screen.findByRole("list", { name: "Chọn giải để quay" });
    const gone = within(list).getByRole("button", { name: /^Hết rồi/ });
    expect(gone).toHaveAttribute("aria-disabled", "true");
    expect(gone).toHaveTextContent("Hết suất");
    expect(within(list).getByRole("button", { name: /^Cổ vai gáy/ })).toHaveAttribute("aria-pressed", "true");
    expect(within(screen.getByRole("region", { name: "Vòng quay" })).getByText("Cổ vai gáy")).toBeInTheDocument();
  });

  it("turns at once, and announces the winner in a popup only once the wheel has stopped (AC4.2–4.3)", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    vi.useFakeTimers();
    fireEvent.click(within(stage).getByRole("button", { name: "Quay" }));
    // The wheel turns at the press, like the prototype: +6 turns in 4.1 s.
    const disk = diskOf(stage);
    expect(disk.style.getPropertyValue("transition")).toContain("4100ms");
    expect(disk.style.getPropertyPriority("transition")).toBe("important");
    expect(disk.style.transform).not.toBe("rotate(0deg)");
    expect(within(stage).getByRole("button", { name: "Quay" })).toHaveAttribute("aria-busy", "true");
    await act(() => vi.advanceTimersByTimeAsync(4000));
    const spin = api.calls.find((c) => c.path.endsWith("/spins"));
    expect(spin?.body).toEqual({ prize_id: "q1" });
    expect(spin?.headers["Idempotency-Key"]).toMatch(/^[0-9a-f-]{36}$/);
    // Answered, but the wheel still turns: nothing shows yet.
    expect(within(stage).getByText("Đang quay…")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    expect(screen.getByText("Chưa có ai trúng. Chọn giải rồi bấm “Quay”: người trúng hiện ở đây.")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(600));
    const popup = screen.getByRole("dialog", { name: "Chúc mừng" });
    expect(popup).toHaveTextContent("Lượt 1");
    expect(within(popup).getByText("Nguyễn Thị Lan")).toHaveClass("ld-winner-name");
    expect(within(popup).getByText("Cổ vai gáy")).toBeInTheDocument();
    expect(popup).toHaveTextContent("Còn 0/1 suất");
    // Not under the wheel (EM): a screen reader hears it from the status line.
    expect(within(stage).getByText("Nguyễn Thị Lan trúng Cổ vai gáy")).toHaveClass("sr-only");
    expect(within(stage).getByRole("button", { name: "Quay tiếp" })).not.toHaveAttribute("aria-busy");
    expect(within(screen.getByRole("list", { name: "Người trúng giải Cổ vai gáy" })).getByText("Nguyễn Thị Lan")).toBeInTheDocument();
    // "Cổ vai gáy" is used up: the stage moved on to the next prize.
    expect(within(screen.getByRole("list", { name: "Chọn giải để quay" })).getByRole("button", { name: /^Gội đầu/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("an answer later than the stop keeps the button busy until it arrives", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    api.slow((c) => (c.path.endsWith("/spins") ? 7000 : 0));
    vi.useFakeTimers();
    fireEvent.click(within(stage).getByRole("button", { name: "Quay" }));
    await act(() => vi.advanceTimersByTimeAsync(5000));
    expect(within(stage).getByRole("button", { name: "Quay" })).toHaveAttribute("aria-busy", "true");
    expect(within(stage).getByText("Đang quay…")).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(2500));
    expect(screen.getByRole("dialog", { name: "Chúc mừng" })).toHaveTextContent("Nguyễn Thị Lan");
  });

  it("ignores a second press while the wheel turns (AC4.4)", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    vi.useFakeTimers();
    const button = within(stage).getByRole("button", { name: "Quay" });
    fireEvent.click(button);
    fireEvent.click(button);
    await act(() => vi.advanceTimersByTimeAsync(5000));
    expect(api.calls.filter((c) => c.path.endsWith("/spins"))).toHaveLength(1);
  });

  it("after a network error says so once the wheel stops, and retries with the same key (AC4.6)", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    api.intercept((c) => (c.path.endsWith("/spins") ? Promise.reject(new TypeError("Failed to fetch")) : undefined));
    vi.useFakeTimers();
    fireEvent.click(within(stage).getByRole("button", { name: "Quay" }));
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(within(stage).queryByRole("alert")).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(3600));
    expect(within(stage).getByRole("alert")).toHaveTextContent("Chưa quay được, kiểm tra mạng rồi bấm “Quay” lại.");
    fireEvent.click(within(stage).getByRole("button", { name: "Quay" }));
    await act(() => vi.advanceTimersByTimeAsync(4600));
    const keys = api.calls.filter((c) => c.path.endsWith("/spins")).map((c) => c.headers["Idempotency-Key"]);
    expect(keys).toHaveLength(2);
    expect(keys[1]).toBe(keys[0]);
    expect(screen.getByRole("dialog", { name: "Chúc mừng" })).toHaveTextContent("Nguyễn Thị Lan");
  });

  it("with reduced motion the wheel still turns, gentler, and the winner shows once it stops", async () => {
    vi.stubGlobal("matchMedia", (query: string) => ({
      matches: query.includes("reduce"),
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    }));
    fakeApi(OWNER, [DRAW]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    vi.useFakeTimers();
    fireEvent.click(within(stage).getByRole("button", { name: "Quay" }));
    const disk = diskOf(stage);
    expect(disk.style.getPropertyValue("transition")).toContain("2500ms");
    expect(disk.style.getPropertyPriority("transition")).toBe("important");
    expect(disk.style.transform).toMatch(/^rotate\((\d+(\.\d+)?)deg\)$/);
    expect(disk.style.transform).not.toBe("rotate(0deg)");
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(screen.getByRole("dialog", { name: "Chúc mừng" })).toHaveTextContent("Nguyễn Thị Lan");
  });
});

describe("winner popup (EM 2026-11-28)", () => {
  async function spunOnce(draw = DRAW) {
    const api = fakeApi(OWNER, [draw]);
    at(`/vong-quay/${D1}`);
    const stage = await screen.findByRole("region", { name: "Vòng quay" });
    vi.useFakeTimers();
    fireEvent.click(within(stage).getByRole("button", { name: /^Quay/ }));
    await act(() => vi.advanceTimersByTimeAsync(4600));
    return { api, stage, popup: screen.getByRole("dialog", { name: "Chúc mừng" }) };
  }

  it("“Quay tiếp” closes it and spins the next prize at once", async () => {
    const { api, popup } = await spunOnce();
    fireEvent.click(within(popup).getByRole("button", { name: "Quay tiếp" }));
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(4600));
    expect(api.calls.filter((c) => c.path.endsWith("/spins")).map((c) => c.body)).toEqual([{ prize_id: "q1" }, { prize_id: "q2" }]);
    expect(screen.getByRole("dialog", { name: "Chúc mừng" })).toHaveTextContent("Lượt 2");
  });

  it("“Vắng mặt, trả giải” goes to the confirm, then the slot comes back", async () => {
    const { api, popup } = await spunOnce();
    fireEvent.click(within(popup).getByRole("button", { name: "Vắng mặt, trả giải" }));
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    const confirm = screen.getByRole("dialog", { name: "Trả giải “Cổ vai gáy” của Nguyễn Thị Lan?" });
    fireEvent.click(within(confirm).getByRole("button", { name: "Vắng mặt, trả giải" }));
    await act(() => vi.advanceTimersByTimeAsync(100));
    expect(api.calls.some((c) => c.path.endsWith("/forfeit"))).toBe(true);
    expect(within(screen.getByRole("list", { name: "Chọn giải để quay" })).getByRole("button", { name: /^Cổ vai gáy/ })).toHaveTextContent("Còn 1/1 suất");
  });

  it("“Đóng” gives focus back to the spin button", async () => {
    const { stage, popup } = await spunOnce();
    expect(within(popup).getByRole("button", { name: "Quay tiếp" })).toHaveFocus();
    fireEvent.click(within(popup).getByRole("button", { name: "Đóng" }));
    await act(() => vi.advanceTimersByTimeAsync(50));
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    expect(within(stage).getByRole("button", { name: "Quay tiếp" })).toHaveFocus();
  });

  it("the spin that finishes the program offers “Xem tổng kết” instead, and only then the summary (AC6.1)", async () => {
    const last = detail({
      ...DRAW,
      participants: [person("p1", "Nguyễn Thị Lan", "female", 0), person("p2", "Trần Văn Minh", "male", 1, "r1")],
      prizes: [prize("q1", "Cổ vai gáy", { eligible: "female", quantity: 1, remaining: 1, eligible_count: 1 }), prize("q2", "Gội đầu", { quantity: 1, remaining: 0, awarded: 1, eligible_count: 1, position: 1 })],
      results: [result("r1", 1, "q2", "p2")],
      status: "in_progress",
    });
    const { popup } = await spunOnce(last);
    expect(within(popup).queryByRole("button", { name: "Quay tiếp" })).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Tổng kết chương trình" })).not.toBeInTheDocument();
    fireEvent.click(within(popup).getByRole("button", { name: "Xem tổng kết" }));
    expect(screen.queryByRole("dialog", { name: "Chúc mừng" })).not.toBeInTheDocument();
    const sheet = screen.getByRole("dialog", { name: "Tổng kết chương trình" });
    const women = within(sheet).getByRole("region", { name: "Cổ vai gáy" });
    expect(within(women).getByText("Nguyễn Thị Lan")).toBeInTheDocument();
    expect(within(women).getByText("Nữ", { selector: ".ld-win .gender-chip" })).toBeInTheDocument();
    expect(within(within(sheet).getByRole("region", { name: "Gội đầu" })).getByText("Trần Văn Minh")).toBeInTheDocument();
  });
});

describe("winners list (EM 2026-11-28: cards grouped by prize)", () => {
  it("groups by prize in prize order, spin order within, with Đã trao a/b; a forfeit stays dimmed", async () => {
    const won = detail({
      ...DRAW,
      participants: DRAW.participants.map((p) => ({ ...p, result_id: { p1: "r2", p2: "r1", p3: "r3" }[p.id] ?? null })),
      prizes: [
        prize("q1", "Cổ vai gáy", { eligible: "female", quantity: 1, remaining: 1, awarded: 0, eligible_count: 0, position: 0 }),
        prize("q2", "Gội đầu", { quantity: 2, remaining: 0, awarded: 2, eligible_count: 1, position: 1 }),
      ],
      results: [result("r1", 1, "q2", "p2"), result("r2", 2, "q1", "p1", true), result("r3", 3, "q2", "p3")],
      status: "in_progress",
    });
    fakeApi(OWNER, [won]);
    at(`/vong-quay/${D1}`);
    const card = await screen.findByRole("region", { name: "Danh sách trúng thưởng" });
    const groups = within(card).getAllByRole("heading", { level: 3 }).map((h) => h.textContent);
    expect(groups).toEqual(["Cổ vai gáy", "Gội đầu"]);
    expect(within(card).getByText("Đã trao 2/2")).toBeInTheDocument();
    const anyone = within(card).getByRole("list", { name: "Người trúng giải Gội đầu" });
    expect(within(anyone).getAllByRole("listitem").map((li) => li.querySelector(".ld-win-who")?.textContent)).toEqual(["Trần Văn Minh", "Lê Thu Hà"]);
    const absent = within(within(card).getByRole("list", { name: "Người trúng giải Cổ vai gáy" })).getByRole("listitem");
    expect(absent).toHaveClass("is-absent");
    expect(within(absent).getByText("Vắng mặt")).toBeInTheDocument();
    expect(within(absent).queryByRole("button")).not.toBeInTheDocument();
    expect(absent.querySelector(".ld-seq")).toHaveTextContent("Lượt 2");
  });
});

describe("forfeit (US5)", () => {
  it("asks first, then marks the result absent and gives the slot back", async () => {
    const won = detail({
      ...DRAW,
      participants: DRAW.participants.map((p) => (p.id === "p1" ? { ...p, result_id: "r1" } : p)),
      results: [result("r1", 1, "q1", "p1")],
      status: "in_progress",
    });
    const api = fakeApi(OWNER, [won]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    const winners = await screen.findByRole("list", { name: "Người trúng giải Cổ vai gáy" });
    await user.click(within(winners).getByRole("button", { name: "Thao tác với lượt 1, Nguyễn Thị Lan" }));
    await user.click(screen.getByRole("menuitem", { name: "Vắng mặt, trả giải" }));
    const sheet = await screen.findByRole("dialog", { name: "Trả giải “Cổ vai gáy” của Nguyễn Thị Lan?" });
    expect(sheet).toHaveTextContent("Suất này quay lại cho lượt sau; Nguyễn Thị Lan không được quay nữa.");
    expect(api.calls.some((c) => c.path.endsWith("/forfeit"))).toBe(false);
    await user.click(within(sheet).getByRole("button", { name: "Vắng mặt, trả giải" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(api.calls.find((c) => c.path.endsWith("/forfeit"))?.path).toBe(`/api/v1/lucky-draws/${D1}/results/r1/forfeit`);
    const row = within(screen.getByRole("list", { name: "Người trúng giải Cổ vai gáy" })).getByRole("listitem");
    expect(row).toHaveClass("is-absent");
    expect(within(row).getByText("Vắng mặt")).toBeInTheDocument();
    expect(within(row).queryByRole("button")).not.toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Chọn giải để quay" })).getByRole("button", { name: /^Cổ vai gáy/ })).toHaveTextContent("Còn 1/1 suất");
  });
});

describe("participants (US2)", () => {
  it("paste: lists every bad line and sends nothing until they are fixed (AC2.2)", async () => {
    const api = fakeApi(OWNER, [detail()]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Dán danh sách" }));
    const sheet = await screen.findByRole("dialog", { name: "Dán danh sách người tham gia" });
    const box = within(sheet).getByLabelText("Danh sách");
    await user.click(box);
    await user.paste("Họ tên, Giới tính\nNguyễn Thị Lan, Nữ\nTrần Văn Minh\nLê Thu Hà\tnu");
    expect(await within(sheet).findByText("Dòng 3: thiếu giới tính — ví dụ: Trần Văn Minh, Nam")).toBeInTheDocument();
    const add = within(sheet).getByRole("button", { name: "Thêm 2 người" });
    expect(add).toHaveAttribute("aria-disabled", "true");
    await user.click(add);
    expect(api.calls.some((c) => c.method === "POST")).toBe(false);
    await user.clear(box);
    await user.paste("Họ tên, Giới tính\nNguyễn Thị Lan, Nữ\nTrần Văn Minh, Nam\nLê Thu Hà\tnu");
    await waitFor(() => {
      expect(within(sheet).queryByText(/Dòng 3/)).not.toBeInTheDocument();
    });
    expect(within(sheet).getByText(/Sẽ thêm/)).toHaveTextContent("Sẽ thêm 3 người (2 nữ, 1 nam).");
    await user.click(within(sheet).getByRole("button", { name: "Thêm 3 người" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(api.calls.find((c) => c.method === "POST")?.body).toEqual({
      participants: [
        { name: "Nguyễn Thị Lan", gender: "female" },
        { name: "Trần Văn Minh", gender: "male" },
        { name: "Lê Thu Hà", gender: "female" },
      ],
    });
    expect(screen.getByRole("table", { name: "Danh sách người tham gia" })).toHaveTextContent("Lê Thu Hà");
  });

  it("paste: a long list goes in batches of 500, one after another", async () => {
    const api = fakeApi(OWNER, [detail()]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Dán danh sách" }));
    const sheet = await screen.findByRole("dialog", { name: "Dán danh sách người tham gia" });
    await user.click(within(sheet).getByLabelText("Danh sách"));
    await user.paste(Array.from({ length: 1100 }, (_, i) => `Khách ${String(i + 1)}, ${i % 2 ? "Nam" : "Nữ"}`).join("\n"));
    await user.click(await within(sheet).findByRole("button", { name: "Thêm 1.100 người" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    const posts = api.calls.filter((c) => c.method === "POST");
    expect(posts.map((c) => (c.body as { participants: unknown[] }).participants.length)).toEqual([500, 500, 100]);
    expect(screen.getByText("Hiện thêm (1.000 dòng nữa)")).toBeInTheDocument();
  });

  it("says what the server refused: a program already full (participants too_many)", async () => {
    const api = fakeApi(OWNER, [detail()]);
    api.intercept((c) =>
      c.method === "POST"
        ? Response.json({ code: "validation_failed", errors: [{ field: "participants", code: "too_many" }] }, { status: 422 })
        : undefined,
    );
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Dán danh sách" }));
    const sheet = await screen.findByRole("dialog", { name: "Dán danh sách người tham gia" });
    await user.click(within(sheet).getByLabelText("Danh sách"));
    await user.paste("Lan, Nữ");
    await user.click(await within(sheet).findByRole("button", { name: "Thêm 1 người" }));
    expect(await within(sheet).findByRole("alert")).toHaveTextContent("Một chương trình có tối đa 2.000 người tham gia.");
  });

  it("filters by chip with counts and searches without accents; only people not drawn have ⋯", async () => {
    const won = detail({
      ...DRAW,
      participants: DRAW.participants.map((p) => (p.id === "p2" ? { ...p, result_id: "r1" } : p)),
      results: [result("r1", 1, "q2", "p2")],
      status: "in_progress",
    });
    fakeApi(OWNER, [won]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    const table = await screen.findByRole("table", { name: "Danh sách người tham gia" });
    expect(within(table).queryByRole("button", { name: "Thao tác với Trần Văn Minh" })).not.toBeInTheDocument();
    expect(within(table).getByRole("button", { name: "Thao tác với Nguyễn Thị Lan" })).toBeInTheDocument();
    expect(within(table).getByText("Đã trúng: Gội đầu")).toBeInTheDocument();
    const chips = screen.getByRole("group", { name: "Lọc người tham gia" });
    await user.click(within(chips).getByRole("button", { name: "Nữ 2" }));
    expect(screen.getByRole("table", { name: "Danh sách người tham gia" })).not.toHaveTextContent("Trần Văn Minh");
    await user.click(within(chips).getByRole("button", { name: "Tất cả 4" }));
    await user.type(screen.getByRole("searchbox", { name: "Tìm theo họ tên" }), "thu ha");
    await waitFor(() => {
      expect(screen.getByRole("table", { name: "Danh sách người tham gia" })).not.toHaveTextContent("Nguyễn Thị Lan");
    });
    expect(screen.getByRole("table", { name: "Danh sách người tham gia" })).toHaveTextContent("Lê Thu Hà");
  });

  it("add one: Nữ / Nam must be chosen; the sheet stays open for the next name", async () => {
    const api = fakeApi(OWNER, [detail()]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Thêm người" }));
    const sheet = await screen.findByRole("dialog", { name: "Thêm người tham gia" });
    await user.type(within(sheet).getByLabelText("Họ tên"), "Ngô Mai");
    await user.click(within(sheet).getByRole("button", { name: "Thêm người" }));
    expect(within(sheet).getByText("Chọn Nữ hoặc Nam.")).toBeInTheDocument();
    expect(api.calls.some((c) => c.method === "POST")).toBe(false);
    await user.click(within(sheet).getByRole("button", { name: "Nữ" }));
    await user.click(within(sheet).getByRole("button", { name: "Thêm người" }));
    expect(await within(sheet).findByText("Đã thêm Ngô Mai. Nhập người tiếp theo, hoặc bấm “Xong”.")).toBeInTheDocument();
    expect(within(sheet).getByLabelText("Họ tên")).toHaveValue("");
    expect(api.calls.find((c) => c.method === "POST")?.body).toEqual({ participants: [{ name: "Ngô Mai", gender: "female" }] });
  });
});

describe("prizes (US3)", () => {
  it("adds a prize for women only, then keeps its audience once drawn", async () => {
    const api = fakeApi(OWNER, [detail()]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Thêm giải thưởng" }));
    const sheet = await screen.findByRole("dialog", { name: "Thêm giải thưởng" });
    await user.type(within(sheet).getByLabelText("Tên giải"), "Chăm sóc da");
    await user.click(within(sheet).getByRole("button", { name: "Chỉ nữ" }));
    await user.clear(within(sheet).getByLabelText("Số suất"));
    await user.type(within(sheet).getByLabelText("Số suất"), "3");
    await user.click(within(sheet).getByRole("button", { name: "Thêm giải thưởng" }));
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    expect(api.calls.find((c) => c.method === "POST")?.body).toEqual({ name: "Chăm sóc da", description: null, eligible: "female", quantity: 3 });
    expect(screen.getByRole("button", { name: /^Chăm sóc da/ })).toHaveTextContent("Còn 3/3 suất");
  });

  it("a drawn prize has no “Xóa giải”, and its audience is fixed in the form (AC3.2–3.3)", async () => {
    const won = detail({
      ...DRAW,
      participants: DRAW.participants.map((p) => (p.id === "p1" ? { ...p, result_id: "r1" } : p)),
      results: [result("r1", 1, "q1", "p1", true)],
      status: "in_progress",
    });
    fakeApi(OWNER, [won]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Thao tác với giải Cổ vai gáy" }));
    expect(screen.queryByRole("menuitem", { name: "Xóa giải" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("menuitem", { name: "Sửa giải" }));
    const sheet = await screen.findByRole("dialog", { name: "Sửa giải Cổ vai gáy" });
    expect(within(sheet).getByRole("button", { name: "Cả nam và nữ" })).toBeDisabled();
    expect(within(sheet).getByText("Giải đã có kết quả quay nên không đổi được “Dành cho”.")).toBeInTheDocument();
  });
});

describe("program (US1, US7)", () => {
  it("delete asks first, then goes back to the list", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Thao tác với chương trình" }));
    await user.click(screen.getByRole("menuitem", { name: "Xóa chương trình" }));
    const sheet = await screen.findByRole("dialog", { name: "Xóa chương trình “Tri ân 20/11”?" });
    expect(sheet).toHaveTextContent("Danh sách người tham gia, giải và kết quả sẽ bị xóa, không khôi phục được.");
    await user.click(within(sheet).getByRole("button", { name: "Xóa chương trình" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Vòng quay may mắn" })).toBeInTheDocument();
    expect(api.calls.some((c) => c.method === "DELETE" && c.path === `/api/v1/lucky-draws/${D1}`)).toBe(true);
    expect(await screen.findByText("Chưa có chương trình nào")).toBeInTheDocument();
  });

  it("rename sends the version it read (If-Match)", async () => {
    const api = fakeApi(OWNER, [DRAW]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    await user.click(await screen.findByRole("button", { name: "Thao tác với chương trình" }));
    await user.click(screen.getByRole("menuitem", { name: "Đổi tên" }));
    const sheet = await screen.findByRole("dialog", { name: "Đổi tên chương trình" });
    const field = within(sheet).getByLabelText("Tên chương trình");
    await user.clear(field);
    await user.type(field, "Tri ân khách quen");
    await user.click(within(sheet).getByRole("button", { name: "Lưu tên" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Tri ân khách quen" })).toBeInTheDocument();
    expect(api.calls.find((c) => c.method === "PUT")?.headers["If-Match"]).toBe('"1"');
  });

  it("presentation covers the screen and gives focus back to its button", async () => {
    fakeApi(OWNER, [DRAW]);
    const user = userEvent.setup();
    at(`/vong-quay/${D1}`);
    const open = await screen.findByRole("button", { name: "Trình chiếu" });
    await user.click(open);
    const show = await screen.findByRole("dialog", { name: "Tri ân 20/11" });
    expect(within(show).getByRole("combobox", { name: "Giải đang quay" })).toHaveValue("q1");
    expect(within(show).getByRole("button", { name: "Quay" })).toBeInTheDocument();
    await user.click(within(show).getByRole("button", { name: "Thoát trình chiếu" }));
    await waitFor(() => {
      expect(open).toHaveFocus();
    });
    expect(screen.queryByRole("dialog", { name: "Tri ân 20/11" })).not.toBeInTheDocument();
  });

  it("an unknown program says so, with the way back", async () => {
    fakeApi(OWNER, []);
    at(`/vong-quay/${D1}`);
    expect(await screen.findByText(/Không tìm thấy chương trình này/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Về danh sách chương trình" })).toHaveAttribute("href", "/vong-quay");
  });

  it("an id that is not an id never reaches the API, and reads the same", async () => {
    const api = fakeApi(OWNER, []);
    at("/vong-quay/nope");
    expect(await screen.findByText(/Không tìm thấy chương trình này/)).toBeInTheDocument();
    expect(api.calls.some((c) => c.path.startsWith("/api/v1/lucky-draws/"))).toBe(false);
  });
});
