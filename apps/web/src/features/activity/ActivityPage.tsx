import type { ActivityEntry, ActivityKind } from "@vpyd/api-client";
import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { DateField } from "../../components/DateField";
import { dayLabel, spaToday } from "../../lib/format";
import { useSearchState } from "../../lib/searchState";
import { FilterSelect, ResponsiveFilters } from "../reports/filters";
import { EmptyState, ErrorState, PageHeader, RowsSkeleton } from "../reports/ui";
import { activityQuery } from "./api";
import { ACTIVITY_KINDS, activitySearch } from "./search";
import {
  KIND_LABEL,
  aboutDay,
  actorName,
  changeText,
  discountOf,
  moneyOf,
  phraseOf,
  subjectName,
  timeOf,
} from "./sentence";
import "./activity.css";

/**
 * /nhat-ky-thao-tac (spec management-views §11, ADR-0021): who did what, when, for the owner.
 * Grouped by day, newest first; one plain sentence per entry with its money, changed values and
 * reason. The route opens for `audit.read` only (UX); `GET /audit-log` refuses everyone else and
 * never sends a note's text.
 */
export function ActivityPage() {
  const [s, patch] = useSearchState(activitySearch);
  const page = useInfiniteQuery(activityQuery(s));
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];
  const first = page.data?.pages[0];
  const people = first?.people ?? [];
  const today = spaToday();
  const actorOptions: (readonly [string, string])[] = [
    ["", "Mọi người"],
    ...people.map((p) => [p.user_id, p.left ? `${p.name} (đã nghỉ)` : p.name] as const),
    ...(s.actor && s.actor !== "platform" && s.actor !== "system" && !people.some((p) => p.user_id === s.actor)
      ? [[s.actor, "Người đã chọn"] as const]
      : []),
    ["platform", "Đội vận hành"],
    ["system", "Hệ thống"],
  ];
  const kindOptions: (readonly [ActivityKind | "", string])[] = [
    ["", "Mọi loại việc"],
    ...ACTIVITY_KINDS.map((k) => [k, KIND_LABEL[k]] as const),
  ];
  const active = [s.from, s.to, s.actor, s.kind].filter(Boolean).length;
  const days = groupByDay(items);

  return (
    <section className="activity-page">
      <PageHeader title="Nhật ký thao tác" />
      <p className="activity-lede">
        Ai đã làm gì trong app, lúc nào: ghi và hủy lượt, thu tiền, bán và hủy gói, lịch hẹn, sửa hồ sơ khách, chấm công, thưởng/phạt, bảng giá, lương, tài khoản. Chỉ chủ spa xem được.
      </p>
      <ResponsiveFilters
        label="Lọc nhật ký"
        activeCount={active}
        onReset={() => {
          patch({ from: "", to: "", actor: "", kind: "" });
        }}
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
          label="Người làm"
          value={s.actor}
          options={actorOptions}
          onChange={(actor) => {
            patch({ actor });
          }}
        />
        <FilterSelect
          label="Loại việc"
          value={s.kind}
          options={kindOptions}
          onChange={(kind) => {
            patch({ kind });
          }}
        />
      </ResponsiveFilters>
      {first && (
        <p className="text-sm text-text-muted" role="status">
          {first.from === first.to ? dayLabel(first.from) : `${dayLabel(first.from)} – ${dayLabel(first.to)}`}
          {page.isSuccess && ` · ${String(items.length)}${page.hasNextPage ? "+" : ""} thao tác`}
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
            <EmptyState title="Chưa có thao tác nào">
              {active > 0
                ? "Không có thao tác nào khớp bộ lọc trong khoảng ngày này. Thử chọn khoảng ngày dài hơn hoặc bỏ bớt bộ lọc."
                : "Trong khoảng ngày này chưa ai làm gì trong app. Thử chọn khoảng ngày dài hơn."}
            </EmptyState>
          </div>
        )}
        {days.map(([day, entries]) => (
          <section key={day} className="grid gap-2" aria-labelledby={`activity-${day}`}>
            <h2 id={`activity-${day}`} className="activity-day">
              {dayLabel(day)}
            </h2>
            <ol className="card activity-list">
              {entries.map((e) => (
                <ActivityRow key={e.id} entry={e} />
              ))}
            </ol>
          </section>
        ))}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button type="button" className="btn btn-secondary min-w-40" disabled={page.isFetchingNextPage} onClick={() => void page.fetchNextPage()}>
            {page.isFetchingNextPage ? "Đang tải…" : "Xem thêm"}
          </button>
        </div>
      )}
    </section>
  );
}

function groupByDay(items: ActivityEntry[]): [string, ActivityEntry[]][] {
  const out: [string, ActivityEntry[]][] = [];
  for (const e of items) {
    const last = out.at(-1);
    if (last && last[0] === e.day) last[1].push(e);
    else out.push([e.day, [e]]);
  }
  return out;
}

/** The subject inside the sentence: a link to the customer's profile where there is one. */
function Subject({ entry: e }: { entry: ActivityEntry }) {
  const name = subjectName(e.subject);
  if (e.subject?.kind === "customer" && e.subject.id) {
    return (
      <Link to="/khach-hang/$customerId" params={{ customerId: e.subject.id }} className="name-link">
        {name}
      </Link>
    );
  }
  return <span className="activity-subject">{name}</span>;
}

/** One entry: "Quản lý Hà hủy lượt của chị Mai", then time · group · money · reason, then the changes. */
function ActivityRow({ entry: e }: { entry: ActivityEntry }) {
  const p = phraseOf(e);
  const day = aboutDay(e);
  const money = [moneyOf(e), discountOf(e)].filter(Boolean).join(", ");
  return (
    <li className="activity-row">
      <p className="activity-sentence">
        <strong>{actorName(e)}</strong> {p.lead}
        {p.about !== false && (
          <>
            {" "}
            <Subject entry={e} />
          </>
        )}
        {day && ` ${day}`}
        {p.tail && ` ${p.tail}`}
      </p>
      <p className="activity-meta">
        <span className="tabular">{timeOf(e)}</span>
        <span className="badge badge-muted">{KIND_LABEL[e.kind]}</span>
        {money && <span className="tabular">· {money}</span>}
        {e.reason && <span className="activity-reason">· lý do: {e.reason}</span>}
      </p>
      {e.changes.length > 0 && (
        <ul className="activity-changes">
          {e.changes.map((c) => (
            <li key={c.field}>{changeText(c)}</li>
          ))}
        </ul>
      )}
    </li>
  );
}
