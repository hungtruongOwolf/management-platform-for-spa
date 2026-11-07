import type { Payslip } from "./api";

type CommissionLine = Payslip["commission_lines"][number];
type UpsellLine = NonNullable<Payslip["upsell_lines"]>[number];

/** One row of a work table: a service (or package) once, with what it counts. */
export interface WorkRow {
  name: string;
  qty: number;
  value: number;
}

/** A block of a work table with its subtotal. */
export interface WorkGroup {
  key: string;
  label: string;
  rows: WorkRow[];
  qty: number;
  value: number;
}

export interface WorkTable {
  groups: WorkGroup[];
  qty: number;
  /** The payslip's commission base: the sum of the groups. */
  value: number;
  commission: number;
  rateBp: number;
}

function group(key: string, label: string, rows: WorkRow[]): WorkGroup {
  const sorted = [...rows].sort((a, b) => b.value - a.value || a.name.localeCompare(b.name, "vi"));
  return {
    key,
    label,
    rows: sorted,
    qty: rows.reduce((n, r) => n + r.qty, 0),
    value: rows.reduce((n, r) => n + r.value, 0),
  };
}

/**
 * Whether the spa counts gift sessions at full price (rule `commission_on_bonus_sessions`): the
 * payslip's base then equals the lines' full value. The rule itself is not on the payslip, so the
 * base tells which value the table must add up to.
 */
export function countsFullValue(lines: CommissionLine[], base: number): boolean {
  const full = lines.reduce((n, l) => n + l.full_value, 0);
  const paid = lines.reduce((n, l) => n + l.value, 0);
  return full !== paid && full === base;
}

/**
 * "Dịch vụ đã làm" grouped by type (EM 2026-11-07): one-off services and package sessions are
 * separate blocks, each service once per block with a subtotal, and the total is the payslip's
 * service-commission base — so the technician sees how the lines become "Hoa hồng làm".
 */
export function serviceWork(slip: Payslip): WorkTable {
  const full = countsFullValue(slip.commission_lines, slip.service_commission_base);
  const rows = (source: string) =>
    slip.commission_lines
      .filter((l) => (source === "package" ? l.source === "package" : l.source !== "package"))
      .map((l) => ({ name: l.service_name, qty: l.qty, value: full ? l.full_value : l.value }));
  const groups = [group("one_off", "Dịch vụ lẻ", rows("one_off")), group("package", "Buổi liệu trình", rows("package"))].filter(
    (g) => g.rows.length > 0,
  );
  return {
    groups,
    qty: groups.reduce((n, g) => n + g.qty, 0),
    value: slip.service_commission_base,
    commission: slip.service_commission,
    rateBp: slip.commission_rate_bp,
  };
}

const UPSELL_GROUPS: [string, string][] = [
  ["package", "Gói liệu trình chốt được"],
  ["service", "Dịch vụ lẻ giới thiệu"],
  // A package refunded this month takes its refund back from whoever closed it (ADR-0010).
  ["package_refund", "Hoàn tiền gói (trừ lại)"],
];

/** "Giới thiệu / chốt được" grouped by kind, its total = the closing-commission base. */
export function upsellWork(slip: Payslip): WorkTable {
  const lines: UpsellLine[] = slip.upsell_lines ?? [];
  const known = new Set(UPSELL_GROUPS.map(([k]) => k));
  const groups = UPSELL_GROUPS.map(([kind, label]) =>
    group(
      kind,
      label,
      lines
        .filter((l) => l.kind === kind || (kind === "service" && !known.has(l.kind)))
        .map((l) => ({ name: l.description, qty: l.qty, value: l.value })),
    ),
  ).filter((g) => g.rows.length > 0);
  return {
    groups,
    qty: groups.filter((g) => g.key !== "package_refund").reduce((n, g) => n + g.qty, 0),
    value: slip.closing_commission_base,
    commission: slip.closing_commission,
    rateBp: slip.commission_rate_bp,
  };
}
