import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField, isoToDmy } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { fieldMessage, messageFor } from "../../lib/errors";
import { ErrorState, RowsSkeleton } from "../reports/ui";
import { accountsQuery, createProfile, invalidateStaff, profilesQuery, savePay, updateProfile, type StaffProfile } from "./api";
import { percentOfBp } from "./labels";
import { vnd } from "./ui";

/** Hồ sơ nhân viên (staff.manage): position, start date, status, login; pay terms with payroll.read. */
export function ProfilesTab({ canPay }: { canPay: boolean }) {
  const query = useQuery(profilesQuery);
  const [editing, setEditing] = useState<StaffProfile | "new" | null>(null);
  if (query.isPending) return <RowsSkeleton rows={5} />;
  if (query.isError) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />;
  const columns: Column<StaffProfile>[] = [
    {
      id: "name",
      header: "Họ tên",
      card: "title",
      cell: (p) => (
        <button
          type="button"
          className="link-button"
          onClick={() => {
            setEditing(p);
          }}
        >
          {p.full_name}
        </button>
      ),
    },
    { id: "position", header: "Vị trí", card: "subtitle", cell: (p) => p.position },
    {
      id: "status",
      header: "Trạng thái",
      card: "aside",
      cell: (p) => (p.status === "active" ? <span className="badge badge-ok">Đang làm</span> : <span className="badge badge-muted">Đã nghỉ {p.left_on ? isoToDmy(p.left_on) : ""}</span>),
    },
    { id: "joined", header: "Ngày vào làm", cell: (p) => (p.joined_on ? isoToDmy(p.joined_on) : "–") },
    { id: "login", header: "Tài khoản", cell: (p) => p.user_name ?? "Chưa gắn" },
    ...(canPay
      ? [
          { id: "base", header: "Lương cứng", numeric: true, cell: (p: StaffProfile) => (p.pay ? vnd(p.pay.base_salary) : "Chưa có") },
          { id: "rate", header: "Hoa hồng", numeric: true, cell: (p: StaffProfile) => (p.pay ? percentOfBp(p.pay.commission_rate_bp) : "–") },
        ]
      : []),
  ];
  return (
    <div className="grid content-start gap-4">
      <div>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => {
            setEditing("new");
          }}
        >
          + Thêm nhân viên
        </button>
      </div>
      <ResponsiveTable caption="Hồ sơ nhân viên" rows={query.data} rowKey={(p) => p.id} columns={columns} />
      {editing && (
        <ProfileSheet
          canPay={canPay}
          profile={editing === "new" ? null : editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

function ProfileSheet({ profile, canPay, onClose }: { profile: StaffProfile | null; canPay: boolean; onClose: () => void }) {
  const queryClient = useQueryClient();
  const accounts = useQuery(accountsQuery);
  const [name, setName] = useState(profile?.full_name ?? "");
  const [position, setPosition] = useState(profile?.position ?? "Kỹ thuật viên");
  const [phone, setPhone] = useState(profile?.phone ?? "");
  const [joined, setJoined] = useState(profile?.joined_on ?? "");
  const [left, setLeft] = useState(profile?.left_on ?? "");
  const [userId, setUserId] = useState(profile?.user_id ?? "");
  const [shift, setShift] = useState(profile?.shift_start ?? "");
  const pay = profile?.pay;
  // The server decides (`pay_visible`); `canPay` only covers a profile not created yet.
  const payVisible = profile ? profile.pay_visible : canPay;
  const [base, setBase] = useState(pay ? String(pay.base_salary) : "");
  const [rate, setRate] = useState(pay ? String(pay.commission_rate_bp / 100) : "5");
  const [loyalty, setLoyalty] = useState(pay ? String(pay.loyalty_bonus_monthly) : "0");
  const [meal, setMeal] = useState(pay ? String(pay.meal_allowance) : "0");
  const digits = (v: string) => Number(v.replace(/\D/g, "") || "0");
  const save = useMutation({
    mutationFn: async () => {
      const body = {
        full_name: name.trim(),
        position: position.trim(),
        phone: phone.trim() || null,
        status: left ? ("left" as const) : ("active" as const),
        joined_on: joined || null,
        left_on: left || null,
        user_id: userId || null,
        shift_start: shift || null,
      };
      const saved = profile ? await updateProfile(profile.id, profile.version, body) : await createProfile(body);
      if (payVisible && base) {
        await savePay(saved.id, {
          base_salary: digits(base),
          commission_rate_bp: Math.round(Number(rate.replace(",", ".")) * 100),
          loyalty_bonus_monthly: digits(loyalty),
          meal_allowance: digits(meal),
        });
      }
    },
    onSuccess: async () => {
      await invalidateStaff(queryClient);
      onClose();
    },
  });
  const err = (f: string) => fieldMessage(save.error, f);
  const formId = "profile-form";
  return (
    <BottomSheet
      open
      wide
      onClose={onClose}
      title={profile ? `Hồ sơ ${profile.full_name}` : "Thêm nhân viên"}
      footer={
        <button type="submit" form={formId} className="btn btn-primary" disabled={save.isPending || !name.trim()}>
          Lưu
        </button>
      }
    >
      <form
        id={formId}
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
      >
        <div className="grid gap-4 sm:grid-cols-2">
          <label className="filter-field">
            Họ tên
            <input required maxLength={200} value={name} onChange={(e) => { setName(e.target.value); }} aria-invalid={err("full_name") ? true : undefined} />
          </label>
          <label className="filter-field">
            Vị trí
            <input required maxLength={100} value={position} onChange={(e) => { setPosition(e.target.value); }} />
          </label>
          <label className="filter-field">
            Số điện thoại
            <input inputMode="tel" value={phone} onChange={(e) => { setPhone(e.target.value); }} aria-invalid={err("phone") ? true : undefined} />
            {err("phone") && <span className="text-sm text-danger">{err("phone")}</span>}
          </label>
          <label className="filter-field">
            Giờ vào ca
            <input type="time" value={shift} onChange={(e) => { setShift(e.target.value); }} />
          </label>
          <DateField label="Ngày vào làm" value={joined} onChange={setJoined} />
          <DateField label="Ngày nghỉ việc (nếu đã nghỉ)" value={left} onChange={setLeft} />
          <label className="filter-field sm:col-span-2">
            Tài khoản đăng nhập (để nhân viên tự xem công và lương)
            <select value={userId} onChange={(e) => { setUserId(e.target.value); }}>
              <option value="">Chưa gắn</option>
              {(accounts.data ?? [])
                .filter((a) => !a.staff_id || a.staff_id === profile?.id)
                .map((a) => (
                  <option key={a.user_id} value={a.user_id}>
                    {a.full_name} ({a.roles.join(", ")})
                  </option>
                ))}
            </select>
          </label>
        </div>
        {err("left_on") && <p className="text-sm text-danger">Ngày nghỉ việc: {err("left_on")}</p>}
        {payVisible ? (
          <fieldset className="grid gap-4 sm:grid-cols-2">
            <legend className="mb-1 text-sm font-semibold">Lương (chỉ chủ spa thấy)</legend>
            <label className="filter-field">
              Lương cứng thực nhận / tháng (đ)
              <input inputMode="numeric" value={base} onChange={(e) => { setBase(e.target.value.replace(/[^\d.]/g, "")); }} />
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
              <input inputMode="numeric" value={loyalty} onChange={(e) => { setLoyalty(e.target.value.replace(/[^\d.]/g, "")); }} />
            </label>
            <label className="filter-field">
              Hỗ trợ tiền ăn / tháng (đ)
              <input inputMode="numeric" value={meal} onChange={(e) => { setMeal(e.target.value.replace(/[^\d.]/g, "")); }} />
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
