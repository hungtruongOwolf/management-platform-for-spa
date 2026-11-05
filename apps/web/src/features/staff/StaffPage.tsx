import { useSuspenseQuery } from "@tanstack/react-query";
import { PageHeader } from "../../app/PageHeader";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { AdjustmentsTab } from "./AdjustmentsTab";
import { AttendanceTab } from "./AttendanceTab";
import { EvaluationTab } from "./EvaluationTab";
import { MyMonth } from "./MyMonth";
import { PayrollTab } from "./PayrollTab";
import { ProfilesTab } from "./ProfilesTab";
import { staffSearch, type StaffTab } from "./search";
import { MonthPicker } from "./ui";
import "./staff.css";

const TAB_LABEL: Record<StaffTab, string> = {
  attendance: "Chấm công",
  evaluation: "Đánh giá",
  adjustments: "Thưởng, phạt",
  payroll: "Bảng lương",
  profiles: "Hồ sơ",
};

/**
 * /nhan-vien (spec staff-management): tabs by permission — Chấm công, Đánh giá, Thưởng/phạt
 * (attendance/evaluation.record), Bảng lương (payroll.read), Hồ sơ (staff.manage). A technician
 * without those sees only their own month. Tabs are UX only: each API answers 403 by itself.
 */
export function StaffPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, patch] = useSearchState(staffSearch);
  const today = spaToday();
  const current = today.slice(0, 7);
  const month = search.month || current;
  const tabs = (
    [
      ["attendance", can(me, "attendance.record")],
      ["evaluation", can(me, "evaluation.record")],
      ["adjustments", can(me, "evaluation.record")],
      ["payroll", can(me, "payroll.read")],
      ["profiles", can(me, "staff.manage")],
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
    return (
      <section className="grid content-start gap-5">
        <PageHeader eyebrow="Nhân viên" title="Công và lương của tôi">
          {picker}
        </PageHeader>
        {can(me, "staff.read_own") ? (
          <MyMonth month={month} />
        ) : (
          <p role="alert" className="rounded-[10px] bg-surface-2 p-4 text-text-muted">
            Màn hình này chưa mở cho tài khoản của bạn.
          </p>
        )}
      </section>
    );
  }

  const onDay = (d: string) => {
    patch({ day: d === today ? "" : d });
  };
  return (
    <section className="grid content-start gap-5">
      <PageHeader eyebrow="Nhân viên" title="Nhân viên" label={`Nhân viên · ${TAB_LABEL[tab]}`}>
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
        {tab === "attendance" && <AttendanceTab month={month} day={search.day || today} today={today} onDay={onDay} />}
        {tab === "evaluation" && <EvaluationTab month={month} day={search.day || today} today={today} onDay={onDay} />}
        {tab === "adjustments" && <AdjustmentsTab month={month} today={today} />}
        {tab === "payroll" && <PayrollTab month={month} canClose={can(me, "payroll.close")} />}
        {tab === "profiles" && <ProfilesTab canPay={can(me, "payroll.read")} />}
      </div>
    </section>
  );
}
