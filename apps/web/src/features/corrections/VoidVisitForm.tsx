import type { Visit } from "@vpyd/api-client";
import { timeOf } from "../front-desk/api";
import { voidVisit } from "./api";
import { VoidForm } from "./VoidForm";

/** "Hủy lượt" form of the /quay day list. Lazily loaded so it stays off the front desk's first paint. */
export function VoidVisitForm({
  visit,
  onDone,
  onCancel,
}: {
  visit: Visit;
  onDone: (voided: Visit) => void;
  onCancel: () => void;
}) {
  return (
    <VoidForm
      title={`Hủy lượt ${timeOf(visit.check_in_at)} của ${visit.customer_name}`}
      consequence="Buổi đã trừ được cộng lại vào liệu trình; tiền đã thu được ghi bút toán đảo. Lượt vẫn còn trong lịch sử với trạng thái đã hủy."
      confirmLabel="Hủy lượt"
      run={(reason, key) => voidVisit(visit.id, reason, key)}
      onDone={onDone}
      onCancel={onCancel}
    />
  );
}
