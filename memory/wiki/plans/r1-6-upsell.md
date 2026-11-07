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

## Phần B — giao diện giỏ (PR "redesign(quay): …", 2026-11-06)
| # | Ticket | Trạng thái | Test |
|---|---|---|---|
| B1 | Giỏ 3 phần ở `/quay`: "Gói bán hôm nay" inline (mẫu gói, người giới thiệu / chốt mặc định KTV của dòng khớp, quà, giảm giá gói ≤ 10 %, "Mua cho người khác"), "Dùng gói này cho buổi hôm nay" (tự bật khi gói chứa dịch vụ lẻ trong giỏ; dòng về 0đ, gạch giá cũ), ô "Giới thiệu" cho dòng lẻ (mặc định Khách tự chọn), "Người đi cùng (ghi chú)"; "Phải trả" + dòng "Dịch vụ lẻ X + gói Y"; chỉ có gói → "Lưu hóa đơn gói" (`POST /sales`); chunk lazy `CartPackages` | ✓ | `round6.test.ts`, `CheckoutRound6.test.tsx`, e2e `quay-upsell` |
| B2 | Giỏ nháp: `DraftKeeper` (lazy) lưu mỗi lần đổi, mở lại giỏ mới nhất sau tải lại + thanh "Đã khôi phục giỏ chưa lưu · Bỏ giỏ", chọn lại khách thì giỏ của khách đó quay về; xóa khi lưu / "Bỏ giỏ" / giỏ rỗng / đăng xuất; không lưu khi đang Sửa lượt | ✓ | vitest + e2e reload |
| B3 | ⋯ của lượt → "Thêm dịch vụ / bán gói" (additions; người giới thiệu mặc định = KTV của lượt) và "Chuyển sang gói liệu trình" (chỉ khi lượt có dòng lẻ; ô "Tính buổi hôm nay vào gói" mặc định tắt → bán gói thường qua additions; bật → convert + If-Match, "Khách đưa thêm …") — form mở ngay dưới dòng (`VisitUpsell`, lazy) | ✓ | vitest + e2e |
| B4 | Hoàn tất lịch hẹn: cùng giỏ (bán gói, đi cùng lấy từ lịch). "Khách đến cả nhóm": "+ Bán gói cho <tên>" mỗi người (`GroupPackagePick`, 1 gói/người) — *bỏ 2026-11-07 cùng sheet nhóm (spec lịch hẹn §10.12)*. "+ Người đi cùng" ở form đặt lẻ + đặt nhóm (`BookingSheet`, gửi `companions`) làm ở PR redesign Lịch hẹn (#59) | ✓ | vitest gián tiếp |
| B5 | Thẻ khách: "Bán gói liệu trình" (mở khối trong giỏ), "Mua lại gói này" trên gói hết buổi (điền sẵn mẫu; mẫu ngừng bán → "Mẫu gói đã ngừng bán, chọn gói khác") | ✓ | vitest |
| B6 | Playwright + axe: `e2e/specs/quay-upsell.spec.ts` (bán gói dùng ngay + chi tiết lượt căn cột + khoảng cách ≥ 8px, giỏ nháp sau tải lại, chuyển sang gói rồi thêm dịch vụ) | ✓ | CI |

## Ngoài phạm vi (ghi lại)
Q-U7 trả trước / công nợ (khi làm: khách nợ không dùng buổi, danh sách nhắc chưa thanh toán) · một người trả cho cả nhóm · dùng chung gói (D5) · "mở lượt" lúc khách vãng lai đến · KTV tự ghi upsell trên điện thoại.
