import { useSuspenseQuery } from "@tanstack/react-query";
import { mayOpen } from "../../app/access";
import { PageHeader } from "../../app/PageHeader";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { DailyBook } from "./DailyBook";
import { EvaluationTab } from "./EvaluationTab";
import { MonthSheet } from "./MonthSheet";
import { MyMonth } from "./MyMonth";
import { PayrollTab } from "./PayrollTab";
import { RosterTab } from "./RosterTab";
import { staffSearch, type StaffTab } from "./search";
import { MonthPicker } from "./ui";
import "./staff.css";

const TAB_LABEL: Record<StaffTab, string> = {
  day: "Sổ hằng ngày",
  month: "Bảng công tháng",
  evaluation: "Đánh giá tháng",
  payroll: "Bảng lương",
  profiles: "Nhân sự",
};

/**
 * /nhan-vien (spec staff-management §10): the daily book first (attendance, praise/complaints and
 * bonuses/penalties per person, in one pass), then the month grid, the monthly evaluation (the
 * book prompts for it when due), payroll (payroll.read) and the roster "Nhân sự" (staff.read;
 * hiring, editing and ending employment with staff.manage, spec §13). A
 * technician without those sees only their own month. Tabs are UX only: each API answers 403.
 */
export function StaffPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, patch] = useSearchState(staffSearch);
  const today = spaToday();
  const current = today.slice(0, 7);
  const month = search.month || current;
  const canAttendance = can(me, "attendance.record");
  const canReview = can(me, "evaluation.record");
  const tabs = (
    [
      ["day", canAttendance || canReview],
      ["month", canAttendance],
      ["evaluation", canReview],
      ["payroll", can(me, "payroll.read")],
      ["profiles", can(me, "staff.read")],
    ] as const
  )
    .filter(([, ok]) => ok)
    .map(([t]) => t);
  const tab = tabs.includes(search.tab) ? search.tab : tabs[0];
  const picker = (
    <MonthPicker
      month={month}
      current={current}
      onChange={(m) => {
        patch({ month: m === current ? "" : m, day: "" });
      }}
    />
  );

  if (!tab) {
    // A leaver (ADR-0014): their own months only; the server picks the month they left by
    // default and refuses the months they did not work in, so the picker lives with the data.
    const former = can(me, "staff.read_own_past") && !can(me, "staff.read_own");
    return (
      <section className="grid content-start gap-5">
        <PageHeader title="Công và lương của tôi">
          {!former && picker}
        </PageHeader>
        {mayOpen.myPay(me) ? (
          <MyMonth
            month={former ? search.month : month}
            former={former}
            onMonth={(m) => {
              patch({ month: m, day: "" });
            }}
            day={search.day}
            onDay={(d) => {
              // The open day is in the URL (ADR-0007); its month follows it.
              patch(d ? { day: d, month: d.slice(0, 7) === current ? "" : d.slice(0, 7) } : { day: "" });
            }}
          />
        ) : (
          <p role="alert" className="note">
            Màn hình này chưa mở cho tài khoản của bạn.
          </p>
        )}
      </section>
    );
  }

  const onDay = (d: string) => {
    patch({ day: d === today ? "" : d, month: d.slice(0, 7) === current ? "" : d.slice(0, 7) });
  };
  return (
    <section className="grid content-start gap-5">
      <PageHeader title="Nhân viên" label={`Nhân viên · ${TAB_LABEL[tab]}`}>
        {tab !== "profiles" && picker}
      </PageHeader>
      <div role="tablist" aria-label="Mục quản lý nhân viên" className="tabs tabs-scroll">
        {tabs.map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            id={`staff-tab-${t}`}
            aria-selected={tab === t}
            aria-controls="staff-panel"
            onClick={() => {
              patch({ tab: t }, { push: true });
            }}
          >
            {TAB_LABEL[t]}
          </button>
        ))}
      </div>
      <div id="staff-panel" role="tabpanel" aria-labelledby={`staff-tab-${tab}`} className="grid content-start gap-5">
        {tab === "day" && (
          <DailyBook
            month={month}
            day={search.day || today}
            today={today}
            canAttendance={canAttendance}
            canReview={canReview}
            onDay={onDay}
            onOpenEvaluation={(m) => {
              patch({ tab: "evaluation", month: m === current ? "" : m, day: "" }, { push: true });
            }}
          />
        )}
        {tab === "month" && <MonthSheet month={month} today={today} canReview={canReview} />}
        {tab === "evaluation" && <EvaluationTab month={month} />}
        {tab === "payroll" && <PayrollTab month={month} canClose={can(me, "payroll.close")} />}
        {tab === "profiles" && (
          <RosterTab
            canManage={can(me, "staff.manage")}
            canPay={can(me, "payroll.read")}
            filter={search.roster}
            onFilter={(f) => {
              patch({ roster: f });
            }}
          />
        )}
      </div>
    </section>
  );
}
