// Round 6 (spec R1-1 §10): the API calls and pure rules behind selling a package in the checkout,
// adding services to a saved visit ("Thêm dịch vụ vào lượt") and "Tính buổi hôm nay vào gói".
// The cart UI that uses them is built on the redesigned /quay (plan r1-6-upsell, part B); keep
// this module free of React so that UI can lazy-load it with its sheets.
import type {
  PackagePlan,
  Visit,
  VisitAdditions,
  VisitConversion,
  VisitCorrected,
} from "@vpyd/api-client";
import { api, ifMatch } from "../../lib/api";

/** `POST /visits/{id}/additions`: new lines and sales; nothing existing changes. */
export async function addToVisit(
  visitId: string,
  body: VisitAdditions,
  idempotencyKey: string,
): Promise<Visit> {
  return (await api(`/visits/${visitId}/additions`, {
    method: "POST",
    headers: { "Idempotency-Key": idempotencyKey },
    body,
  })) as Visit;
}

/** `POST /visits/{id}/convert-to-package`: today's covered one-off lines become session 1. */
export async function convertToPackage(
  visit: Pick<Visit, "id" | "version">,
  body: VisitConversion,
  idempotencyKey: string,
): Promise<VisitCorrected> {
  return (await api(`/visits/${visit.id}/convert-to-package`, {
    method: "POST",
    headers: { ...ifMatch(visit.version), "Idempotency-Key": idempotencyKey },
    body,
  })) as VisitCorrected;
}

/** One-off lines of the visit that the plan covers: what "Tính buổi hôm nay vào gói" converts. */
export function coveredLines(visit: Visit, plan: PackagePlan): Visit["lines"] {
  const services = new Set(plan.items.map((i) => i.service_id));
  return visit.lines.filter(
    (l) => l.source === "one_off" && services.has(l.service_id),
  );
}

/**
 * The choice is offered only when it means something (Q-U1, case by case): the visit is the
 * current version and the plan covers a one-off service done today. Default stays "off".
 */
export function canCountToday(visit: Visit, plan: PackagePlan): boolean {
  return (
    visit.status === "completed" &&
    !visit.corrected_by_visit_id &&
    coveredLines(visit, plan).length > 0
  );
}

/**
 * Money of a conversion as the desk shows it, before sending: `due` is what the conversion costs
 * (the package after its discount + the one-off lines kept), `handOver` what the customer gives
 * now ("Khách đưa thêm"): `due` minus what was paid today, which the server records again in its
 * own method. The request's payments are `handOver` only (none when it is 0 or less).
 */
export function conversionMoney(
  visit: Visit,
  plan: PackagePlan,
  packageDiscount = 0,
): { due: number; handOver: number } {
  const converted = new Set(coveredLines(visit, plan).map((l) => l.id));
  const kept = visit.lines
    .filter((l) => l.source === "one_off" && !converted.has(l.id))
    .reduce((s, l) => s + l.line_value, 0);
  const due = plan.price - packageDiscount + kept;
  return { due, handOver: due - visit.total };
}

/**
 * Default "người giới thiệu / chốt" (Q-U2): the technician serving the customer for a sale made
 * during a visit (the line the package covers, else the first line); "khách tự chọn" (null) for a
 * plain desk sale. The desk may always change it.
 */
export function defaultCloser(
  lines: { serviceId: string; staffId: string }[],
  plan: Pick<PackagePlan, "items"> | null,
): string | null {
  if (lines.length === 0) return null;
  const covered = new Set(plan?.items.map((i) => i.service_id) ?? []);
  const match = lines.find((l) => covered.has(l.serviceId) && l.staffId);
  return (match ?? lines[0])?.staffId || null;
}
