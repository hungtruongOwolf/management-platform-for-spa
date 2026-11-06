---
title: "Plan vòng 6: upsell trong lượt, chuyển lượt lẻ sang gói"
status: in-progress
updated: 2026-11-06
sources: [raw/2026-11-06-em-upsell-and-convert-to-package.md, raw/2026-11-06-em-answers-round6-upsell.md]
related: [../product/specs/front-desk-and-packages.md, ../architecture/backend.md, ../architecture/data.md, ../design/front-desk-flows.md]
---

# Plan vòng 6: upsell trong lượt, chuyển lượt lẻ sang gói

Spec: [front-desk-and-packages §10](../product/specs/front-desk-and-packages.md) (approved 2026-11-06, quyết định §10.8). Thiết kế: [backend §3e](../architecture/backend.md), [data 0021](../architecture/data.md), [front-desk-flows vòng 6](../design/front-desk-flows.md).

**Phối hợp:** PR #56 "redesign(pilot): /quay" đang sửa `FrontDeskPage`, `CustomerCard`, giỏ, `TodayVisits`, design-system. Phần backend + hợp đồng API + lương làm trước (PR vòng 6, phần A). Giao diện giỏ (phần B) chỉ build **sau khi #56 merge**, trên component và quy tắc mới.

## Phần A — backend, migration, lương, hợp đồng (PR vòng 6)
| # | Ticket | Trạng thái | Test |
|---|---|---|---|
| A1 | Migration 0021: `sale.visit_id` (FK deferrable, backfill, trigger), `visit_line.sale_id` + `referrer_staff_id`, closer nullable, `companions`, `revenue_book` thêm `visit_id` + `upsell_by` | ✓ | `upsell.rs`, migrate trên DB trống + DB có dữ liệu cũ (backfill) |
| A2 | `sales`: tách `price_package_sale` / `record_package_sale` (POST /sales dùng lại), `price_services`, `allocate_payments` (hàm thuần), `beneficiary_customer_id`, người chốt nullable | ✓ | unit `allocate_payments` (4), `sales.rs` |
| A3 | `operations::checkout`: `package_sales[]` + `package_ref` + `referrer_staff_id` + `companions` trong `POST /visits`, `/appointments/{id}/complete` | ✓ | AC9.1–9.13 |
| A4 | `POST /visits/{id}/additions` | ✓ | AC12.1–12.5 |
| A5 | `POST /visits/{id}/convert-to-package` (begin/finish_correction, payroll lock) | ✓ | AC11.1–11.9 |
| A6 | Hủy lượt đảo mọi hóa đơn lẻ của lượt; gói giữ | ✓ | AC12.4 |
| A7 | Lương: `upsell_lines`, `upsell_package_base` / `upsell_service_base`, phiếu cũ đọc được | ✓ | unit calc (2) + AC-P1, AC-P2 |
| A8 | Báo cáo: sổ doanh thu `upsell_by` + `visit_id`; lịch sử khách gộp hóa đơn lẻ của lượt, `beneficiary_name`, `visit_id` | ✓ | `upsell.rs` (ledger, history), `reports.rs` |
| A9 | Lịch hẹn: `companions` (đặt lẻ, đặt nhóm 1 khách + ghi chú), hoàn tất chép sang lượt | ✓ | AC-C1 |
| A10 | Authz matrix: 2 route mới; OpenAPI + `pnpm api:types`; `.sqlx` | ✓ | `authz_matrix.rs` |
| A11 | Web hooks không đụng file của pilot: API client mới (`features/front-desk/upsell.ts`), giỏ nháp `sessionStorage` (`draftCart.ts`), "Khách tự chọn" ở bán gói, phiếu lương hiện hoa hồng giới thiệu (gói / dịch vụ), sổ doanh thu cột "Giới thiệu" | ✓ | vitest |

## Phần B — giao diện giỏ (sau khi #56 merge)
| # | Ticket | Phụ thuộc |
|---|---|---|
| B1 | Giỏ 3 phần ở `/quay`: "Gói bán hôm nay" inline (mẫu gói, người giới thiệu mặc định KTV của dòng khớp, quà, giảm giá gói ≤ 10 %), "Dùng gói này cho buổi hôm nay" (tự bật khi gói chứa dịch vụ lẻ trong giỏ), cột "Người giới thiệu" cho dòng lẻ (mặc định Khách tự chọn), "Người đi cùng (ghi chú)"; tổng = lẻ + gói; lazy chunk | #56 |
| B2 | Giỏ nháp: nối `draftCart.ts` vào giỏ (lưu mỗi lần đổi, xóa khi lưu / hủy / đăng xuất) | B1 |
| B3 | "Lượt khách hôm nay" ⋯ → "Thêm dịch vụ / bán gói" (additions) và "Bán gói cho lượt này" + ô "Tính buổi hôm nay vào gói" (convert, chỉ hiện khi gói chứa dịch vụ lẻ của lượt; mặc định tắt) với dòng "Khách đưa thêm …" | B1 |
| B4 | Hoàn tất lịch hẹn + "Khách đến cả nhóm": khối bán gói thu gọn mỗi người; đặt nhóm: "+ Người đi cùng (ghi chú)" | B1 |
| B5 | Thẻ khách: "Bán gói liệu trình", "Mua lại gói này" (điền sẵn mẫu) | B1 |
| B6 | Playwright + axe: bán gói trong giỏ dùng ngay, chuyển sang gói, thêm dịch vụ, giỏ nháp sau tải lại | B1–B5 |

## Ngoài phạm vi (ghi lại)
Q-U7 trả trước / công nợ (khi làm: khách nợ không dùng buổi, danh sách nhắc chưa thanh toán) · một người trả cho cả nhóm · dùng chung gói (D5) · "mở lượt" lúc khách vãng lai đến · KTV tự ghi upsell trên điện thoại.
