import { addMessages } from "../../lib/errors";

// Refusals only the void / cancel forms can get. They live with those (lazy) forms, not in the
// startup chunk (frontend.md "Perf budget"); every form that can trigger them imports this module.
addMessages(
  {
    "sale.voided": "Hóa đơn này đã được hủy trước đó.",
    "sale.has_used_sessions": 'Gói đã dùng buổi nên không hủy hóa đơn được. Hãy dùng "Hủy gói và hoàn tiền".',
    "sale.void_via_visit": "Hóa đơn dịch vụ lẻ được hủy cùng lượt khách.",
    "refund.exceeds_paid": "Số tiền hoàn lớn hơn số khách đã trả cho liệu trình.",
    "package.restore_exceeds_total": "Buổi của lượt này đã được cộng lại trước đó. Vui lòng tải lại.",
    // ADR-0019 D8: another technician on a visit of a month whose payroll is closed.
    "payroll.period_closed":
      "Tháng của lượt này đã chốt lương nên không đổi kỹ thuật viên hay thêm vào lượt được nữa.",
  },
  {},
);
