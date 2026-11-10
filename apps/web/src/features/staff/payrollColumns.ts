import type { Payslip, PayrollMonth } from "./api";

/**
 * The money columns of one row of the payroll table (a payslip, or the month's totals). They add
 * up to Thực lĩnh: `base − deductions + commission + meal + adjustments + shortfall = net`, where
 * `shortfall` is what deductions exceeded the month's pay by and was not deducted (net pay is
 * never negative, ADR-0019 D3; flagged "Khoản trừ vượt lương").
 */
export interface PayColumns {
  base: number;
  /** Leave + late deductions, as a positive amount. */
  deductions: number;
  commission: number;
  meal: number;
  /** Bonuses − penalties. */
  adjustments: number;
  shortfall: number;
  net: number;
}

export function slipColumns(s: Payslip): PayColumns {
  return {
    base: s.base_pay,
    deductions: s.leave_deduction + s.late_deduction,
    commission: s.service_commission + s.closing_commission,
    meal: s.meal_allowance,
    adjustments: s.bonuses - s.penalties,
    shortfall: s.net_pay_shortfall ?? 0,
    net: s.net_pay,
  };
}

export function totalColumns(t: PayrollMonth["totals"]): PayColumns {
  return {
    base: t.base_salary,
    deductions: t.deductions,
    commission: t.commission,
    meal: t.meal_allowance,
    adjustments: t.bonuses - t.penalties,
    shortfall: t.net_pay_shortfall,
    net: t.net_pay,
  };
}

/** What the columns add up to; equals `net` for every payslip the server computes. */
export function columnsTotal(c: PayColumns): number {
  return c.base - c.deductions + c.commission + c.meal + c.adjustments + c.shortfall;
}
