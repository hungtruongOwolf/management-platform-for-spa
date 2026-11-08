import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { Icon } from "../../app/Icon";
import { DateField } from "../../components/DateField";
import { dayLabel, spaTime } from "../../lib/format";
import { useSearchState } from "../../lib/searchState";
import { spaToday } from "../corrections/api";
import { FilterSelect, ResponsiveFilters, SearchField } from "../reports/filters";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "../reports/ui";
import { activeStaffQuery } from "../sales/api";
import type { HistoryKind } from "@vpyd/api-client";
import { notesQuery, type NoteEntry, type NoteKind } from "./api";
import { NOTE_KINDS, notesSearch } from "./search";
import "./notes.css";

/** Type chip of each kind: its words, and a chip colour by what it is about. */
export const KIND_LABEL: Record<NoteKind, string> = {
  visit_note: "Ghi chú buổi",
  customer_note: "Ghi chú hồ sơ",
  health_note: "Lưu ý sức khỏe",
  attendance_note: "Chấm công",
  review_note: "Khen / phàn nàn",
  adjustment: "Thưởng / phạt",
  follow_up: "Chăm sóc lại",
  void: "Hủy / sửa",
  employment_end: "Nghỉ việc",
  appointment_note: "Ghi chú lịch hẹn",
  evaluation_note: "Đánh giá tháng",
  staff_note: "Ghi chú nhân sự",
  package_extension: "Gia hạn gói",
  discount: "Giảm giá",
  appointment_reason: "Hủy lịch / không đến",
};

const KIND_BADGE: Record<NoteKind, string> = {
  visit_note: "badge-info",
  customer_note: "badge-muted",
  health_note: "badge-warn",
  attendance_note: "badge-muted",
  review_note: "badge-gold",
  adjustment: "badge-ok",
  follow_up: "badge-info",
  void: "badge-danger",
  employment_end: "badge-muted",
  appointment_note: "badge-info",
  evaluation_note: "badge-gold",
  staff_note: "badge-muted",
  package_extension: "badge-info",
  discount: "badge-warn",
  appointment_reason: "badge-danger",
};

/** The chip of one note: some kinds say more precisely what happened (`detail`). */
function chipOf(n: NoteEntry): { label: string; badge: string } {
  if (n.kind === "adjustment" && n.detail === "penalty") return { label: "Phạt", badge: "badge-danger" };
  if (n.kind === "adjustment" && n.detail === "bonus") return { label: "Thưởng", badge: "badge-ok" };
  if (n.kind === "void") {
    const label = { visit: "Hủy lượt", visit_corrected: "Sửa lượt", sale: "Hủy hóa đơn", package: "Hủy gói" }[n.detail ?? ""];
    if (label) return { label, badge: "badge-danger" };
  }
  // F21: why a booking was closed without a visit.
  if (n.kind === "appointment_reason")
    return { label: n.detail === "no_show" ? "Không đến" : "Hủy lịch", badge: "badge-danger" };
  if (n.kind === "evaluation_note" && n.detail === "coaching_plan") return { label: "Kế hoạch kèm cặp", badge: "badge-gold" };
  if (n.kind === "evaluation_note") return { label: "Nhận xét tháng", badge: "badge-gold" };
  return { label: KIND_LABEL[n.kind], badge: KIND_BADGE[n.kind] };
}

const dmy = (iso: string) => iso.split("-").reverse().join("/");

/** The day the note is about, when it is not the day it is filed under. */
function aboutOf(n: NoteEntry): string {
  if (!n.about_date) return "";
  if (n.kind === "appointment_note" || n.kind === "appointment_reason") return `Lịch hẹn ${dmy(n.about_date)}`;
  if (n.kind === "evaluation_note") return `Tháng ${dmy(n.about_date.slice(0, 7))}`;
  if (n.kind === "package_extension") return `Hạn mới ${dmy(n.about_date)}`;
  return "";
}


/** When it was written, in words that say what the time means. */
function whenOf(n: NoteEntry): string {
  if (n.kind === "employment_end") return "Ngày làm cuối";
  const time = spaTime(n.at);
  // EM Q-N2: profile, health and HR notes keep one value on the profile; its last change is shown.
  return n.kind === "customer_note" || n.kind === "health_note" || n.kind === "staff_note" ? `Hồ sơ sửa lúc ${time}` : time;
}

/**
 * /nhat-ky-ghi-chu (spec management-views §10): every note written in the app, newest first, for
 * the owner and the manager to read back. Grouped by business day; each note shows its type, time,
 * author, who it is about and a link back to where it was written. Filters live in the URL. What
 * each kind shows is decided by the server (`GET /notes`, one rule per kind); this screen never
 * receives a note the caller may not read.
 */
export function NotesPage() {
  return (
    <ManagementOnly>
      <NotesPageBody />
    </ManagementOnly>
  );
}

function NotesPageBody() {
  const [s, patch] = useSearchState(notesSearch);
  const page = useInfiniteQuery(notesQuery(s));
  const staff = useQuery(activeStaffQuery);
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];
  const first = page.data?.pages[0];
  const kinds = first?.kinds ?? [];
  const capped = page.data?.pages.at(-1)?.scan_capped ?? false;
  const today = spaToday();
  const customerName = s.customer ? (items.find((n) => n.customer_id === s.customer)?.customer_name ?? "Khách đã chọn") : "";
  const staffOptions: (readonly [string, string])[] = [
    ["", "Mọi nhân viên"],
    ...(staff.data ?? []).map((p) => [p.id, p.full_name] as const),
    ...(s.staff && !staff.data?.some((p) => p.id === s.staff) ? [[s.staff, "Nhân viên đã chọn"] as const] : []),
  ];
  const kindOptions: (readonly [NoteKind | "", string])[] = [
    ["", "Mọi loại"],
    ...NOTE_KINDS.filter((k) => kinds.includes(k) || k === s.kind).map((k) => [k, KIND_LABEL[k]] as const),
  ];
  const active = [s.from, s.to, s.kind, s.staff].filter(Boolean).length;
  const days = groupByDay(items);

  return (
    <section className="notes-page">
      <PageHeader title="Nhật ký ghi chú" />
      <p className="notes-lede">
        Mọi ghi chú đã ghi trong app, mới nhất trước: ghi chú buổi, hồ sơ, lịch hẹn, chấm công, khen/phàn nàn, đánh giá, thưởng/phạt, chăm sóc lại, lý do hủy, gia hạn, giảm giá. Chỉ hiện loại ghi chú tài khoản của bạn được xem.
      </p>
      <ResponsiveFilters
        label="Lọc ghi chú"
        activeCount={active}
        onReset={() => {
          patch({ from: "", to: "", kind: "", staff: "" });
        }}
        search={
          <SearchField
            label="Tìm trong ghi chú"
            placeholder="Chữ, tên khách, nhân viên…"
            value={s.q}
            onCommit={(q) => {
              patch({ q });
            }}
          />
        }
      >
        <DateField
          label="Từ ngày"
          value={s.from || (first?.from ?? "")}
          max={s.to || today}
          onChange={(from) => {
            patch({ from });
          }}
        />
        <DateField
          label="Đến ngày"
          value={s.to || (first?.to ?? "")}
          min={s.from || undefined}
          max={today}
          onChange={(to) => {
            patch({ to });
          }}
        />
        <FilterSelect
          label="Loại"
          value={s.kind}
          options={kindOptions}
          onChange={(kind) => {
            patch({ kind });
          }}
        />
        <FilterSelect
          label="Nhân viên"
          value={s.staff}
          options={staffOptions}
          onChange={(v) => {
            patch({ staff: v });
          }}
        />
      </ResponsiveFilters>
      {s.customer && (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-text-muted">Chỉ ghi chú về:</span>
          <button
            type="button"
            className="notes-filter-chip"
            aria-label={`Bỏ lọc theo khách ${customerName}`}
            onClick={() => {
              patch({ customer: "" });
            }}
          >
            {customerName}
            <Icon name="x" className="size-4" />
          </button>
        </div>
      )}
      {first && (
        <p className="text-sm text-text-muted" role="status">
          {first.from === first.to ? dayLabel(first.from) : `${dayLabel(first.from)} – ${dayLabel(first.to)}`}
          {page.isSuccess && ` · ${String(items.length)}${page.hasNextPage ? "+" : ""} ghi chú`}
        </p>
      )}

      <div className={`grid gap-4 ${page.isPlaceholderData ? "is-refreshing" : ""}`}>
        {page.isError && <ErrorState error={page.error} onRetry={() => void page.refetch()} />}
        {page.isPending && (
          <div className="card">
            <RowsSkeleton rows={6} />
          </div>
        )}
        {page.isSuccess && items.length === 0 && (
          <div className="card">
            <EmptyState title="Không có ghi chú nào">
              {s.q ? `Không thấy ghi chú nào có “${s.q}” trong khoảng ngày này. Thử chọn khoảng ngày dài hơn hoặc bỏ bớt bộ lọc.` : "Trong khoảng ngày này chưa ai ghi chú gì. Thử chọn khoảng ngày dài hơn."}
            </EmptyState>
          </div>
        )}
        {days.map(([day, notes]) => (
          <section key={day} className="grid gap-2" aria-labelledby={`notes-${day}`}>
            <h2 id={`notes-${day}`} className="notes-day">
              {dayLabel(day)}
            </h2>
            <ol className="card notes-list">
              {notes.map((n) => (
                <NoteRow
                  key={`${n.kind}-${n.at}-${n.visit_id ?? n.customer_id ?? n.staff_id ?? ""}-${n.text.slice(0, 16)}`}
                  note={n}
                  onCustomer={
                    n.customer_id && n.customer_id !== s.customer
                      ? () => {
                          patch({ customer: n.customer_id ?? "" }, { push: true });
                        }
                      : undefined
                  }
                />
              ))}
            </ol>
          </section>
        ))}
      </div>
      {capped && (
        <p role="status" className="note">
          Đã tìm trong 500 ghi chú gần nhất của khoảng ngày này. Bấm “Tìm tiếp” để tìm xa hơn.
        </p>
      )}
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button type="button" className="btn btn-secondary min-w-40" disabled={page.isFetchingNextPage} onClick={() => void page.fetchNextPage()}>
            {page.isFetchingNextPage ? "Đang tải…" : capped ? "Tìm tiếp" : "Xem thêm"}
          </button>
        </div>
      )}
    </section>
  );
}

function groupByDay(items: NoteEntry[]): [string, NoteEntry[]][] {
  const out: [string, NoteEntry[]][] = [];
  for (const n of items) {
    const last = out.at(-1);
    if (last && last[0] === n.business_date) last[1].push(n);
    else out.push([n.business_date, [n]]);
  }
  return out;
}

/** One note: chip · time · author, who it is about, the text, the way back to its source. */
function NoteRow({ note: n, onCustomer }: { note: NoteEntry; onCustomer: (() => void) | undefined }) {
  const chip = chipOf(n);
  return (
    <li className="note-row">
      <p className="note-meta">
        <span className={`badge ${chip.badge}`}>{chip.label}</span>
        <span className="tabular">{whenOf(n)}</span>
        {n.author_name && <span>· {n.author_name}</span>}
        {aboutOf(n) && <span className="tabular">· {aboutOf(n)}</span>}
      </p>
      <p className="note-about">
        {n.customer_id && n.customer_name && (
          <Link to="/khach-hang/$customerId" params={{ customerId: n.customer_id }} className="name-link">
            {n.customer_name}
          </Link>
        )}
        {!n.customer_id && n.customer_name && <span className="font-semibold">{n.customer_name}</span>}
        {n.staff_id && n.staff_name && <span className="font-semibold">{n.staff_name}</span>}
        {n.kind === "package_extension" && n.detail && <span className="text-text-muted">{n.detail}</span>}
      </p>
      <p className="note-text">{n.text}</p>
      <p className="note-source">
        <SourceLink note={n} />
        {onCustomer && (
          <button type="button" className="note-link" onClick={onCustomer}>
            <Icon name="filter" className="size-4" />
            Chỉ xem khách này
          </button>
        )}
      </p>
    </li>
  );
}

/** Where the customer's timeline shows each customer kind ("" = the profile, no filter). */
function historyKindOf(n: NoteEntry): HistoryKind | "" {
  if (n.kind === "visit_note") return "visit";
  if (n.kind === "void") return n.detail === "visit" || n.detail === "visit_corrected" ? "visit_voided" : "";
  if (n.kind === "package_extension") return "package_extended";
  if (n.kind === "discount") return n.detail === "visit" ? "visit" : "purchase";
  return "";
}

function SourceLink({ note: n }: { note: NoteEntry }): ReactNode {
  const arrow = <Icon name="arrow-right" className="size-4" />;
  const day = n.business_date;
  if (n.kind === "appointment_note" || n.kind === "appointment_reason") {
    return (
      <Link to="/quay" search={{ view: "lich", ngay: n.about_date ?? day }} className="note-link">
        Mở lịch hẹn
        {arrow}
      </Link>
    );
  }
  const inHistory = historyKindOf(n);
  if (n.customer_id && (inHistory || n.kind === "void")) {
    return (
      <Link
        to="/khach-hang/$customerId"
        params={{ customerId: n.customer_id }}
        search={{ hfrom: day, hto: day, ...(inHistory ? { hkind: inHistory } : {}) }}
        className="note-link"
      >
        {n.kind === "visit_note" ? "Xem lượt khách" : "Xem trong lịch sử khách"}
        {arrow}
      </Link>
    );
  }
  if (n.customer_id) {
    return (
      <Link to="/khach-hang/$customerId" params={{ customerId: n.customer_id }} className="note-link">
        Mở hồ sơ khách
        {arrow}
      </Link>
    );
  }
  if (n.kind === "employment_end" || n.kind === "staff_note") {
    return (
      <Link to="/nhan-vien" search={{ tab: "profiles", roster: "all" }} className="note-link">
        Mở Nhân sự
        {arrow}
      </Link>
    );
  }
  if (n.kind === "evaluation_note") {
    return (
      <Link to="/nhan-vien" search={{ tab: "evaluation", month: (n.about_date ?? day).slice(0, 7) }} className="note-link">
        Mở đánh giá tháng
        {arrow}
      </Link>
    );
  }
  if (n.staff_id) {
    return (
      <Link to="/nhan-vien" search={{ tab: "day", day, month: day.slice(0, 7) }} className="note-link">
        Mở sổ ngày
        {arrow}
      </Link>
    );
  }
  return null;
}
