import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useRef, useState } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField, isoToDmy } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { TimeField } from "../../components/TimeField";
import { ApiError } from "../../lib/api";
import { spaDateTime } from "../../lib/format";
import { fieldMessage, messageFor } from "../../lib/errors";
import { spaToday } from "../corrections/api";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import {
  createProfile,
  endEmployment,
  historyQuery,
  invalidateStaff,
  profilesQuery,
  LOGIN_ROLE_LABEL,
  rehire,
  savePay,
  updateProfile,
  type LoginRole,
  type StaffHistoryEntry,
  type StaffProfile,
} from "./api";
import { percentOfBp } from "./labels";
import { LoginSection, type PendingLogin } from "./LoginSection";
import { shiftDay } from "./MyDaySheet";
import { POSITIONS, ROSTER_FILTERS, ROSTER_FILTER_LABEL, inFilter, tenureLabel, type RosterFilter } from "./roster";
import { vnd } from "./ui";

/**
 * "Hồ sơ nhân viên" (spec staff-management §13): the roster — who works here, since when, for how long,
 * who has left and why — with hiring, editing, ending employment (soft: history stays) and
 * rehiring. Owner (`staff.manage`) writes; a manager (`staff.read`) reads names, positions,
 * periods and tenure only: the server sends nothing more (no phone, note, login or reason).
 */
export function RosterTab({
  canManage,
  canPay,
  filter,
  onFilter,
  onHistory,
}: {
  canManage: boolean;
  canPay: boolean;
  filter: RosterFilter;
  onFilter: (f: RosterFilter) => void;
  /** Opens a person's evaluation history (only for `evaluation.record`; absent otherwise). */
  onHistory?: (id: string) => void;
}) {
  const query = useQuery(profilesQuery);
  const [openId, setOpenId] = useState<string | null>(null);
  // A login asked for in "Thêm nhân viên": created from the new profile's sheet right away.
  const [pendingLogin, setPendingLogin] = useState<PendingLogin | null>(null);
  if (query.isPending) return <RowsSkeleton rows={5} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const everyone = query.data;
  const rows = everyone.filter((p) => inFilter(p, filter));
  // `"new"` = the add form; otherwise a profile id.
  const open = openId === "new" ? null : everyone.find((p) => p.id === openId);
  const columns: Column<StaffProfile>[] = [
    {
      id: "name",
      header: "Họ tên",
      card: "title",
      cell: (p) => (
        <button
          type="button"
          className="name-link"
          onClick={() => {
            setOpenId(p.id);
          }}
        >
          {p.full_name}
        </button>
      ),
    },
    { id: "position", header: "Vị trí", card: "subtitle", cell: (p) => p.position },
    { id: "status", header: "Trạng thái", card: "aside", cell: (p) => <StatusChip profile={p} /> },
    { id: "joined", header: "Ngày vào làm", cell: (p) => (p.joined_on ? isoToDmy(p.joined_on) : "Chưa rõ") },
    { id: "tenure", header: "Thâm niên", cell: (p) => tenureLabel(p) },
    {
      id: "login",
      header: "Tài khoản",
      cell: (p) => (
        <span className="grid gap-1">
          <span>{p.details?.user_name ?? (p.login_linked ? "Đã gắn" : "Chưa gắn")}</span>
        </span>
      ),
    },
    ...(canPay
      ? [
          { id: "base", header: "Lương cứng", numeric: true, cell: (p: StaffProfile) => (p.pay ? vnd(p.pay.base_salary) : "Chưa có") },
          { id: "rate", header: "Hoa hồng", numeric: true, cell: (p: StaffProfile) => (p.pay ? percentOfBp(p.pay.commission_rate_bp) : "–") },
        ]
      : []),
  ];
  return (
    <div className="grid content-start gap-4">
      <div className="roster-toolbar">
        <div role="group" aria-label="Lọc theo trạng thái" className="segmented">
          {ROSTER_FILTERS.map((f) => (
            <button
              key={f}
              type="button"
              aria-pressed={filter === f}
              onClick={() => {
                onFilter(f);
              }}
            >
              {ROSTER_FILTER_LABEL[f]} ({everyone.filter((p) => inFilter(p, f)).length})
            </button>
          ))}
        </div>
        {canManage && (
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setOpenId("new");
            }}
          >
            <Icon name="plus" className="size-[18px]" />
            Thêm nhân viên
          </button>
        )}
      </div>
      {rows.length === 0 ? (
        <p className="note">{filter === "left" ? "Chưa có ai nghỉ việc." : "Chưa có nhân viên nào."}</p>
      ) : (
        <div className="card overflow-hidden">
          <ResponsiveTable caption="Danh sách nhân sự" className="profiles-table" rows={rows} rowKey={(p) => p.id} columns={columns} />
        </div>
      )}
      {openId === "new" && (
        <ProfileForm
          profile={null}
          canPay={canPay}
          onClose={() => {
            setOpenId(null);
          }}
          onCreated={(id, login) => {
            setPendingLogin(login);
            setOpenId(id);
          }}
        />
      )}
      {open && (
        <StaffSheet
          key={open.id}
          profile={open}
          canManage={canManage}
          canPay={canPay}
          onClose={() => {
            setOpenId(null);
            setPendingLogin(null);
          }}
          pendingLogin={pendingLogin}
          {...(onHistory ? { onHistory } : {})}
        />
      )}
    </div>
  );
}

function StatusChip({ profile: p }: { profile: StaffProfile }) {
  return p.status === "active" ? (
    <span className="badge badge-ok">Đang làm</span>
  ) : (
    <span className="badge badge-muted">Đã nghỉ{p.left_on ? ` từ ${isoToDmy(p.left_on)}` : ""}</span>
  );
}

type Mode = "view" | "edit" | "end" | "rehire";

/** One person: periods, tenure, login, history; the owner edits, ends or rehires from here. */
function StaffSheet({
  profile: p,
  canManage,
  canPay,
  onClose,
  onHistory,
  pendingLogin = null,
}: {
  profile: StaffProfile;
  canManage: boolean;
  canPay: boolean;
  onClose: () => void;
  onHistory?: (id: string) => void;
  pendingLogin?: PendingLogin | null;
}) {
  const [mode, setMode] = useState<Mode>("view");
  if (mode === "edit") return <ProfileForm profile={p} canPay={canPay} onClose={onClose} onBack={() => { setMode("view"); }} />;
  if (mode === "end") return <EndSheet profile={p} onClose={onClose} onBack={() => { setMode("view"); }} />;
  if (mode === "rehire") return <RehireSheet profile={p} onClose={onClose} onBack={() => { setMode("view"); }} />;
  const footer = canManage ? (
    <div className="flex flex-wrap gap-2">
      {p.status === "active" ? (
        <button
          type="button"
          className="btn btn-danger-outline"
          onClick={() => {
            setMode("end");
          }}
        >
          Cho nghỉ việc
        </button>
      ) : (
        <button
          type="button"
          className="btn btn-secondary"
          onClick={() => {
            setMode("rehire");
          }}
        >
          Nhận lại làm
        </button>
      )}
      <button
        type="button"
        className="btn btn-primary"
        onClick={() => {
          setMode("edit");
        }}
      >
        Sửa hồ sơ
      </button>
    </div>
  ) : undefined;
  return (
    <BottomSheet open wide onClose={onClose} title={p.full_name} description={p.position} footer={footer}>
      <div className="grid gap-5">
        <dl className="roster-facts">
          <div>
            <dt>Trạng thái</dt>
            <dd>
              <StatusChip profile={p} />
            </dd>
          </div>
          <div>
            <dt>Thâm niên</dt>
            <dd>{tenureLabel(p)}</dd>
          </div>
          <div>
            <dt>Ngày vào làm</dt>
            <dd>{p.joined_on ? isoToDmy(p.joined_on) : "Chưa rõ"}</dd>
          </div>
          {p.first_joined_on && p.first_joined_on !== p.joined_on && (
            <div>
              <dt>Vào làm lần đầu</dt>
              <dd>{isoToDmy(p.first_joined_on)}</dd>
            </div>
          )}
          {p.details && (
            <div>
              <dt>Số điện thoại</dt>
              <dd>{p.details.phone ?? "Chưa có"}</dd>
            </div>
          )}
          <div>
            <dt>Giờ vào ca</dt>
            <dd>{p.shift_start ?? "Theo giờ chung của spa"}</dd>
          </div>
        </dl>
        {onHistory && (
          <button
            type="button"
            className="btn btn-secondary justify-self-start"
            onClick={() => {
              onClose();
              onHistory(p.id);
            }}
          >
            Lịch sử đánh giá
          </button>
        )}
        {p.details?.note && (
          <section className="grid gap-1">
            <h3 className="panel-title">Ghi chú</h3>
            <p className="whitespace-pre-line">{p.details.note}</p>
          </section>
        )}
        <section className="grid gap-2" aria-labelledby="roster-periods">
          <h3 id="roster-periods" className="panel-title">
            Quá trình làm việc
          </h3>
          <ol className="roster-periods">
            {[...p.periods].reverse().map((period, i) => (
              <li key={`${period.started_on ?? "start"}-${String(i)}`}>
                <span className="tabular-nums">
                  {period.started_on ? isoToDmy(period.started_on) : "Chưa rõ"} – {period.ended_on ? isoToDmy(period.ended_on) : "nay"}
                </span>
                {period.ended_on ? (
                  <span className="text-text-muted">{period.end_reason ? `Nghỉ: ${period.end_reason}` : "Đã nghỉ"}</span>
                ) : (
                  <span className="badge badge-ok">Đang làm</span>
                )}
              </li>
            ))}
          </ol>
        </section>
        {canManage && <LoginSection profile={p} pending={pendingLogin} />}
        {canManage && <HistorySection id={p.id} />}
      </div>
    </BottomSheet>
  );
}

const FIELD_LABEL: Record<string, string> = {
  full_name: "Họ tên",
  position: "Vị trí",
  has_phone: "Số điện thoại",
  has_note: "Ghi chú",
  status: "Trạng thái",
  joined_on: "Ngày vào làm",
  left_on: "Ngày nghỉ việc",
  user_id: "Tài khoản",
  shift_start: "Giờ vào ca",
  base_salary: "Lương cứng",
  commission_rate_bp: "Hoa hồng",
  loyalty_bonus_monthly: "Thưởng gắn bó / tháng",
  meal_allowance: "Tiền ăn / tháng",
};

const ACTION_LABEL: Record<string, string> = {
  "staff.create": "Thêm hồ sơ",
  "staff.update": "Sửa hồ sơ",
  "staff.account_link": "Đổi tài khoản đăng nhập",
  "staff.end": "Nghỉ việc",
  "staff.rehire": "Nhận lại làm",
  "staff.pay_update": "Đổi điều kiện lương",
};

function shown(field: string, value: unknown): string {
  if (value === null || value === undefined) return "trống";
  if (field === "has_phone" || field === "has_note") return value === true ? "có" : "không";
  if (field === "status") return value === "left" ? "Đã nghỉ" : "Đang làm";
  if (field === "user_id") return "đã gắn";
  if (field === "commission_rate_bp" && typeof value === "number") return percentOfBp(value);
  if (typeof value === "number") return vnd(value);
  if (typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value)) return isoToDmy(value);
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** Changes that say something to a person (audit keys like `login_locked` are left out). */
function readable(entry: StaffHistoryEntry) {
  return entry.changes.filter((c) => c.field in FIELD_LABEL);
}

function HistorySection({ id }: { id: string }) {
  const query = useQuery(historyQuery(id));
  return (
    <section className="grid gap-2" aria-labelledby="roster-history">
      <h3 id="roster-history" className="panel-title">
        Lịch sử thay đổi
      </h3>
      {query.isPending && <RowsSkeleton rows={2} />}
      {query.isError && <ErrorState error={query.error} onRetry={() => void query.refetch()} />}
      {query.data && query.data.length === 0 && <p className="text-sm text-text-muted">Chưa có thay đổi nào.</p>}
      {query.data && query.data.length > 0 && (
        <ol className="roster-history">
          {query.data.map((h, i) => (
            <li key={`${h.at}-${String(i)}`}>
              <p>
                <strong>{ACTION_LABEL[h.action] ?? h.action}</strong>
                <span className="text-text-muted">
                  {" "}
                  · {spaDateTime(h.at)}
                  {h.actor_name ? ` · ${h.actor_name}` : ""}
                </span>
              </p>
              {h.reason && <p className="text-sm">Lý do: {h.reason}</p>}
              {h.action !== "staff.create" && (
                <ul className="text-sm text-text-muted">
                  {readable(h).map((c) => (
                    <li key={c.field}>
                      {FIELD_LABEL[c.field]}: {shown(c.field, c.before)} → {shown(c.field, c.after)}
                    </li>
                  ))}
                </ul>
              )}
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

const digits = (v: string) => Number(v.replace(/\D/g, "") || "0");

/** Server bounds of the pay terms (org/src/profile.rs `validate_pay`), named in its errors. */
const PAY_MAX: Record<string, number> = {
  base_salary: 1_000_000_000,
  loyalty_bonus_monthly: 100_000_000,
  meal_allowance: 100_000_000,
};

/** Add (hire) or edit a profile; pay terms only when the server says pay is visible. */
function ProfileForm({
  profile,
  canPay,
  onClose,
  onBack,
  onCreated,
}: {
  profile: StaffProfile | null;
  canPay: boolean;
  onClose: () => void;
  onBack?: () => void;
  /** A new profile was saved with "Tạo tài khoản đăng nhập ngay": open its sheet to create the login. */
  onCreated?: (id: string, login: PendingLogin) => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(profile?.full_name ?? "");
  const [position, setPosition] = useState(profile?.position ?? "Kỹ thuật viên");
  const [phone, setPhone] = useState(profile?.details?.phone ?? "");
  const [joined, setJoined] = useState(profile ? (profile.joined_on ?? "") : spaToday());
  const [shift, setShift] = useState(profile?.shift_start ?? "");
  const [note, setNote] = useState(profile?.details?.note ?? "");
  const [withLogin, setWithLogin] = useState(false);
  const [loginPhone, setLoginPhone] = useState("");
  const [loginRole, setLoginRole] = useState<LoginRole>("technician");
  const pay = profile?.pay;
  // The server decides (`pay_visible`); `canPay` only covers a profile not created yet.
  const payVisible = profile ? profile.pay_visible : canPay;
  const [base, setBase] = useState(pay ? String(pay.base_salary) : "");
  const [rate, setRate] = useState(pay ? String(pay.commission_rate_bp / 100) : "5");
  const [loyalty, setLoyalty] = useState(pay ? String(pay.loyalty_bonus_monthly) : "0");
  const [meal, setMeal] = useState(pay ? String(pay.meal_allowance) : "0");
  // The profile and its pay are two writes. When the pay is refused, the profile is already saved:
  // the retry updates that one (its id and version) instead of hiring the person twice or sending
  // a version the server has moved past.
  const written = useRef<StaffProfile | null>(null);
  const save = useMutation({
    mutationFn: async () => {
      const body = {
        full_name: name.trim(),
        position: position.trim(),
        phone: phone.trim() || null,
        joined_on: joined || null,
        shift_start: shift || null,
        note: note.trim() || null,
      };
      const target = written.current ?? profile;
      const saved = target ? await updateProfile(target.id, target.version, body) : await createProfile({ ...body, user_id: null });
      written.current = saved;
      if (payVisible && base) {
        await savePay(saved.id, {
          base_salary: digits(base),
          commission_rate_bp: Math.round(Number(rate.replace(",", ".")) * 100),
          loyalty_bonus_monthly: digits(loyalty),
          meal_allowance: digits(meal),
        });
      }
      return saved;
    },
    onSuccess: async (saved) => {
      await invalidateStaff(queryClient);
      // A login asked for: open the new profile's sheet, where it is created (and shown once).
      if (!profile && onCreated && withLogin && loginPhone.trim()) onCreated(saved.id, { phone: loginPhone.trim(), role: loginRole });
      else onClose();
    },
    onError: async () => {
      // Part of it was saved: show it behind the sheet.
      if (written.current) await invalidateStaff(queryClient);
    },
  });
  const err = (f: string) => fieldMessage(save.error, f);
  const payErr = (f: string) => {
    const message = err(f);
    const max = PAY_MAX[f];
    return message && max !== undefined ? `Tối đa ${vnd(max)}.` : message;
  };
  const formId = "profile-form";
  const leftAlready = profile?.status === "left";
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={profile ? `Sửa hồ sơ ${profile.full_name}` : "Thêm nhân viên"}
      footer={
        <div className="flex flex-wrap gap-2">
          {onBack && (
            <button type="button" className="btn btn-secondary" onClick={onBack}>
              Quay lại
            </button>
          )}
          <button type="submit" form={formId} className="btn btn-primary" aria-busy={save.isPending || undefined} disabled={!name.trim() || !position.trim()}>
            Lưu hồ sơ
          </button>
        </div>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <label className="filter-field">
            Họ tên
            <input required maxLength={200} value={name} onChange={(e) => { setName(e.target.value); }} aria-invalid={err("full_name") ? true : undefined} />
            {err("full_name") && <span className="text-sm text-danger">{err("full_name")}</span>}
          </label>
          <label className="filter-field">
            Vị trí
            {/* EM 2026-11-08: a linked profile's position is its login's role, not free text. */}
            <input
              required
              maxLength={100}
              list="roster-positions"
              value={position}
              readOnly={profile?.position_from_account}
              aria-describedby={profile?.position_from_account ? "roster-position-hint" : undefined}
              onChange={(e) => {
                setPosition(e.target.value);
              }}
            />
            {profile?.position_from_account && (
              <span id="roster-position-hint" className="text-sm text-text-muted">
                Theo vai trò của tài khoản đăng nhập.
              </span>
            )}
            <datalist id="roster-positions">
              {POSITIONS.map((p) => (
                <option key={p} value={p} />
              ))}
            </datalist>
          </label>
          <label className="filter-field">
            Số điện thoại (không bắt buộc)
            <input inputMode="tel" value={phone} onChange={(e) => { setPhone(e.target.value); }} aria-invalid={err("phone") ? true : undefined} />
            {err("phone") && <span className="text-sm text-danger">{err("phone")}</span>}
          </label>
          <label className="filter-field">
            Giờ vào ca
            <TimeField value={shift} onChange={setShift} />
          </label>
          {leftAlready ? (
            <p className="note sm:col-span-2">Đã nghỉ việc: ngày vào làm của đợt đã kết thúc không sửa được. Dùng &quot;Nhận lại làm&quot; để mở đợt mới.</p>
          ) : (
            <DateField label={profile && profile.periods.length > 1 ? "Ngày vào làm (đợt hiện tại)" : "Ngày vào làm"} value={joined} onChange={setJoined} invalid={Boolean(err("joined_on"))} />
          )}
          {profile === null && (
            <fieldset className="grid gap-3 sm:col-span-2">
              <label className="choice">
                <input
                  type="checkbox"
                  checked={withLogin}
                  onChange={(e) => {
                    setWithLogin(e.target.checked);
                  }}
                />
                Tạo tài khoản đăng nhập ngay
              </label>
              {withLogin && (
                <div className="grid gap-3 sm:grid-cols-2">
                  <label className="filter-field">
                    Số điện thoại đăng nhập
                    <input inputMode="tel" autoComplete="off" required value={loginPhone} onChange={(e) => { setLoginPhone(e.target.value); }} />
                  </label>
                  <label className="filter-field">
                    Vai trò
                    <select value={loginRole} onChange={(e) => { setLoginRole(e.target.value as LoginRole); }}>
                      <option value="technician">{LOGIN_ROLE_LABEL.technician}</option>
                      <option value="manager">{LOGIN_ROLE_LABEL.manager}</option>
                    </select>
                  </label>
                </div>
              )}
            </fieldset>
          )}
          <label className="filter-field sm:col-span-2">
            Ghi chú (chỉ chủ spa thấy)
            <textarea rows={3} maxLength={1000} value={note} onChange={(e) => { setNote(e.target.value); }} />
          </label>
        </div>
        {err("joined_on") && <p className="text-sm text-danger">Ngày vào làm: {err("joined_on")}</p>}
        {payVisible ? (
          <fieldset className="grid gap-4 sm:grid-cols-2">
            <legend className="mb-1 text-sm font-semibold">Lương (chỉ chủ spa thấy)</legend>
            <label className="filter-field">
              Lương cứng thực nhận / tháng (đ)
              <input inputMode="numeric" value={base} onChange={(e) => { setBase(e.target.value.replace(/[^\d.]/g, "")); }} aria-invalid={payErr("base_salary") ? true : undefined} />
              {payErr("base_salary") && <span className="text-sm text-danger">{payErr("base_salary")}</span>}
            </label>
            <label className="filter-field">
              Hoa hồng (%)
              <select value={rate} onChange={(e) => { setRate(e.target.value); }}>
                <option value="5">5% (Lựa chọn 1)</option>
                <option value="7">7% (Lựa chọn 2)</option>
                <option value="10">10% (Lựa chọn 3)</option>
                <option value="0">0% (thử việc / học viên)</option>
                {!["5", "7", "10", "0"].includes(rate) && <option value={rate}>{rate}%</option>}
              </select>
            </label>
            <label className="filter-field">
              Thưởng gắn bó / tháng (đ)
              <input inputMode="numeric" value={loyalty} onChange={(e) => { setLoyalty(e.target.value.replace(/[^\d.]/g, "")); }} aria-invalid={payErr("loyalty_bonus_monthly") ? true : undefined} />
              {payErr("loyalty_bonus_monthly") && <span className="text-sm text-danger">{payErr("loyalty_bonus_monthly")}</span>}
            </label>
            <label className="filter-field">
              Hỗ trợ tiền ăn / tháng (đ)
              <input inputMode="numeric" value={meal} onChange={(e) => { setMeal(e.target.value.replace(/[^\d.]/g, "")); }} aria-invalid={payErr("meal_allowance") ? true : undefined} />
              {payErr("meal_allowance") && <span className="text-sm text-danger">{payErr("meal_allowance")}</span>}
            </label>
          </fieldset>
        ) : null}
        {save.isError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}

/** Nghỉ việc: last day + reason, confirmed. Soft: payroll, visits and payslips stay. */
function EndSheet({ profile: p, onClose, onBack }: { profile: StaffProfile; onClose: () => void; onBack: () => void }) {
  const queryClient = useQueryClient();
  const today = spaToday();
  const [on, setOn] = useState(today);
  const [reason, setReason] = useState("");
  const [lockLogin, setLockLogin] = useState(false);
  const save = useMutation({
    mutationFn: () => endEmployment(p.id, p.version, on, reason.trim(), lockLogin),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onBack();
    },
  });
  const formId = "end-form";
  const dateError = fieldMessage(save.error, "ended_on");
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Cho ${p.full_name} nghỉ việc`}
      footer={
        <div className="flex flex-wrap gap-2">
          <button type="button" className="btn btn-secondary" onClick={onBack}>
            Thôi
          </button>
          <button type="submit" form={formId} className="btn btn-danger" disabled={!reason.trim() || !on} aria-busy={save.isPending || undefined}>
            Xác nhận nghỉ việc
          </button>
        </div>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <ul className="grid gap-1 text-sm">
          <li>Từ sau ngày này, {p.full_name} không còn trong danh sách chọn KTV, đặt lịch, sổ hằng ngày và đánh giá các tháng sau.</li>
          <li>Lịch sử chấm công, lượt khách, hoa hồng và phiếu lương giữ nguyên. Có thể nhận lại làm sau.</li>
          {p.login_linked && (
            <li>
              {lockLogin
                ? `Tài khoản đăng nhập của ${p.full_name} sẽ bị khóa ở spa này.`
                : `${p.full_name} vẫn đăng nhập được, nhưng chỉ xem ngày công và phiếu lương của các tháng đã làm.`}
            </li>
          )}
        </ul>
        {p.login_linked && (
          <label className="choice">
            <input
              type="checkbox"
              checked={lockLogin}
              onChange={(e) => {
                setLockLogin(e.target.checked);
              }}
            />
            Khóa tài khoản (không cho đăng nhập nữa)
          </label>
        )}
        <DateField label="Ngày làm cuối cùng" value={on} max={today} onChange={setOn} invalid={Boolean(dateError)} />
        {dateError && <p className="text-sm text-danger">{dateError}</p>}
        <label className="filter-field">
          Lý do nghỉ việc
          <textarea
            rows={3}
            maxLength={500}
            required
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
            }}
          />
        </label>
        {save.isError && !dateError && (
          <div role="alert" className="grid gap-2 text-sm text-danger">
            <p>{messageFor(save.error)}</p>
            <UpcomingBookings error={save.error} staffId={p.id} />
          </div>
        )}
      </form>
    </BottomSheet>
  );
}

interface UpcomingBooking {
  id: string;
  date: string;
  time: string;
  customer: string;
}

/** The bookings of a `staff.has_upcoming_bookings` refusal (server-listed, at most 20). */
function upcomingOf(error: unknown): { bookings: UpcomingBooking[]; more: boolean } | null {
  if (!(error instanceof ApiError) || error.code !== "staff.has_upcoming_bookings") return null;
  const d = error.details as { bookings?: unknown; more?: unknown } | undefined;
  if (!d || !Array.isArray(d.bookings)) return null;
  const bookings = d.bookings.filter(
    (b): b is UpcomingBooking =>
      typeof b === "object" && b !== null && typeof (b as UpcomingBooking).date === "string" && typeof (b as UpcomingBooking).time === "string",
  );
  return { bookings, more: d.more === true };
}

/**
 * F24 (live test 2026-11-08): which bookings still hold the person — day, time, customer — each
 * opening that day of the calendar on their column, where the owner changes the KTV or cancels.
 */
function UpcomingBookings({ error, staffId }: { error: unknown; staffId: string }) {
  const upcoming = upcomingOf(error);
  if (!upcoming || upcoming.bookings.length === 0) return null;
  return (
    <>
      <ul className="grid gap-1 text-text" aria-label="Lịch hẹn còn gán cho nhân viên">
        {upcoming.bookings.map((b) => (
          <li key={b.id}>
            <Link to="/quay" search={{ view: "lich", ngay: b.date, ktv: staffId }} className="link">
              {isoToDmy(b.date)} lúc {b.time} · {b.customer}
            </Link>
          </li>
        ))}
      </ul>
      {upcoming.more && <p className="text-text-muted">Còn lịch khác sau các lịch trên.</p>}
    </>
  );
}

function RehireSheet({ profile: p, onClose, onBack }: { profile: StaffProfile; onClose: () => void; onBack: () => void }) {
  const queryClient = useQueryClient();
  // The new period starts after the last one ended: today, or the day after a same-day end.
  const earliest = p.left_on ? shiftDay(p.left_on, 1) : spaToday();
  const [on, setOn] = useState(earliest > spaToday() ? earliest : spaToday());
  const save = useMutation({
    mutationFn: () => rehire(p.id, p.version, on),
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onBack();
    },
  });
  const formId = "rehire-form";
  const dateError = fieldMessage(save.error, "started_on");
  return (
    <BottomSheet
      open
      onClose={onClose}
      title={`Nhận ${p.full_name} lại làm`}
      footer={
        <div className="flex flex-wrap gap-2">
          <button type="button" className="btn btn-secondary" onClick={onBack}>
            Thôi
          </button>
          <button type="submit" form={formId} className="btn btn-primary" disabled={!on} aria-busy={save.isPending || undefined}>
            Nhận lại làm
          </button>
        </div>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (!save.isPending) save.mutate();
        }}
      >
        <p className="text-sm">
          Mở một đợt làm việc mới; các đợt trước giữ nguyên.{p.login_linked ? " Tài khoản đăng nhập được mở lại đầy đủ quyền." : ""}
        </p>
        <DateField label="Ngày vào làm lại" value={on} min={earliest} onChange={setOn} invalid={Boolean(dateError)} />
        {dateError && <p className="text-sm text-danger">{dateError}</p>}
        {save.isError && !dateError && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(save.error)}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
