---
title: "Plan R1-2: Xem lại & theo dõi"
updated: 2026-11-05
sources: [product/specs/management-views.md, raw/2026-11-05-em-management-views-logo-ui.md]
---

# Plan R1-2: Xem lại & theo dõi

Spec: [management-views](../product/specs/management-views.md) (draft, chờ EM trả lời Q1–Q4). Backend và UI làm song song trên contract `apps/api/openapi.json`.

| # | Việc | Ai | Trạng thái |
|---|---|---|---|
| M1 | Contract: crate `reporting`, 8 endpoint GET, `openapi.json` + `schema.ts` (commit sớm trên `feat/management-views`) | backend | ✅ |
| M2 | Migration 0013: `report.read`, `report.revenue.read`; bỏ `opening_balance.import` của manager; `package_status()`, view `customer_package_state`, `revenue_book`; index theo ngày | backend | ✅ |
| M3 | Bỏ `origin` khỏi `CustomerPackageDto`; `POST /opening-balances` chỉ chủ spa (tạm) | backend | ✅ |
| M4 | CLI `app import-paper-book --tenant --file [--dry-run]` | backend | ✅ |
| M5 | Test: `reports.rs` (9: tiền với giảm giá / hủy / hủy ngày hôm sau / hoàn, 00:00 ICT, quyền tiền, cách ly tenant, phân trang, đối soát `seed-demo`), `paper_book.rs` (2), matrix +8 dòng, bench 3 năm (`reports_bench.rs`, `--ignored`) | backend | ✅ |
| M6 | UI: dashboard Hôm nay (trang chủ), báo cáo khoảng ngày + biểu đồ, sổ doanh thu, hồ sơ khách (timeline, trang liệu trình, tóm tắt), theo dõi liệu trình + lâu không quay lại | UI (`feat/ui-overhaul`) | ⏳ |
| M7 | UI: bỏ `/nhap-so-cu`, nhãn "Nhập từ sổ cũ", ô trang chủ; `PackageActions` dùng `sale_id != null` thay `origin === "sale"`; `CustomerCard.tsx`, `CustomerPackages.tsx` bỏ nhãn | UI | ⏳ |
| M8 | Xóa `POST /opening-balances` + `OpeningBalance*` khỏi contract sau M7 | backend | ⏳ (sau M7) |
| M9 | E2E + axe cho các màn mới; thêm các GET mới vào `api-latency.spec.ts` | UI | ⏳ |
| M10 | Sổ thu chi (phiếu chi, tồn quỹ, chốt quỹ): spec riêng R1-3 | — | để sau |

Thứ tự merge: PR backend trước (UI build trên contract). PR backend làm web typecheck đỏ ở 3 chỗ dùng `origin` (M7) cho tới khi nhánh UI rebase; nếu cần xanh ngay thì UI làm M7 trước.
