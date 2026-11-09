# architecture/

- `system.md`: C4 context/containers, trust boundary, danh sách module (phần hạ tầng là thiết kế VPS gốc; staging chạy Vercel + Supabase).
- `principles.md`: nguyên tắc dễ mở rộng / dễ đổi và hiện trạng của từng nguyên tắc trong code.
- `backend.md`: hợp đồng API (endpoint, mã lỗi, idempotency, rate limit), layering, đồ thị crate, job nền, CLI.
- `data.md`: tra cứu schema (bảng, constraint, trigger, hàm, migration 0001–0040).
- `frontend.md`: route, rendering, state.
- `platform-admin.md`: console admin nền tảng (đăng nhập operator, `/api/v1/admin/*`, các hàm `app.platform_*`).
- `db-review-2026-11.md`, `performance-latency-2026-11.md`: báo cáo rà soát có ngày, kèm trạng thái từng đề xuất.

Viết bởi `/architect`, `/backend-design`, `/frontend-design`, `/perf`.
