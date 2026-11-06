// "Khách đến cả nhóm" (spec R1-1 §10.4.1): each person may buy a package in their own checkout,
// collapsed behind "+ Bán gói". One package per person here (the full cart handles more); the
// closer defaults to the technician serving them. Lives with the calendar's chunk.
import type { Staff } from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { formatVnd } from "@vpyd/ui";
import { useId } from "react";
import { Icon } from "../../app/Icon";
import { plansQuery } from "../catalog/api";
import type { CartLine, PackageSaleDraft } from "./api";
import {
  ReferrerOptions,
  coversCart,
  newSale,
  sessionsText,
} from "./CartPackages";
import { SELF_CHOSEN } from "./checkout";

export function GroupPackagePick({
  person,
  sale,
  lines,
  staffId,
  staff,
  onChange,
}: {
  person: string;
  sale: PackageSaleDraft | null;
  lines: CartLine[];
  /** The technician chosen for this person: the default closer. */
  staffId: string;
  staff: Staff[];
  onChange: (sale: PackageSaleDraft | null) => void;
}) {
  const id = useId();
  const plans = useQuery(plansQuery(false));
  if (!sale)
    return (
      <button
        type="button"
        className="btn btn-ghost"
        onClick={() => {
          onChange({ ...newSale([]), closer: staffId || SELF_CHOSEN });
        }}
      >
        <Icon name="plus" />
        Bán gói cho {person}
      </button>
    );
  return (
    <div className="grid gap-2">
      <div className="sale-fields">
        <label className="field" htmlFor={`${id}-plan`}>
          <span>Gói bán cho {person}</span>
          <select
            id={`${id}-plan`}
            className="input"
            value={sale.plan?.id ?? ""}
            onChange={(e) => {
              const p = plans.data?.find((x) => x.id === e.target.value);
              const plan = p
                ? { id: p.id, name: p.name, price: p.price, items: p.items }
                : null;
              onChange({
                ...sale,
                plan,
                giftNote: p?.gift_note ?? "",
                useToday: coversCart(plan, lines),
              });
            }}
          >
            <option value="">Chọn gói…</option>
            {plans.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} · {formatVnd(p.price)} ({sessionsText(p)})
              </option>
            ))}
          </select>
        </label>
        <label className="field" htmlFor={`${id}-closer`}>
          <span>Người giới thiệu / chốt</span>
          <select
            id={`${id}-closer`}
            className="input"
            value={sale.closer === SELF_CHOSEN ? "" : sale.closer}
            onChange={(e) => {
              onChange({ ...sale, closer: e.target.value || SELF_CHOSEN });
            }}
          >
            <ReferrerOptions staff={staff} />
          </select>
        </label>
      </div>
      {coversCart(sale.plan, lines) && (
        <label className="check-row">
          <input
            type="checkbox"
            checked={sale.useToday}
            onChange={(e) => {
              onChange({ ...sale, useToday: e.target.checked });
            }}
          />
          <span>Dùng gói này cho buổi hôm nay</span>
        </label>
      )}
      <button
        type="button"
        className="btn btn-ghost"
        onClick={() => {
          onChange(null);
        }}
      >
        Bỏ gói
      </button>
    </div>
  );
}
