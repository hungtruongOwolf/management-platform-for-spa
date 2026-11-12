---
title: Project overview
updated: 2026-11-09
sources: [raw/2026-11-04-kickoff-request.md, raw/2026-11-04-idea-internal-platform.md]
---

# Viên Phúc Y Đường — Overview

**Giai đoạn hiện tại (2026-11-12):** build + qa liên tục trên staging (Vercel `pdx1` + Supabase `us-west-2`). Đã build: R1-1 quầy + gói liệu trình, R1-2 màn quản lý + báo cáo, R1-3 lịch hẹn, quản lý nhân sự / chấm công / đánh giá / lương, tài khoản + mật khẩu (ADR-0016), rà soát DB 2026-11 (0033–0037), console nền tảng (ADR-0018, 0038–0040), nhật ký thao tác (ADR-0021, 0051; màn tạm ẩn sau cờ `activity_log`), lịch sử ai ghi / ai sửa ngay trên từng bản ghi (ADR-0022, 0052; đợt 1–2 lên staging 2026-11-12; đợt 3 — bảng giá, mẫu gói, quy tắc lương, số khách tối đa — đã build 2026-11-12, chưa lên staging). Chưa có: sổ thu chi, công nợ, nhắc Zalo/SMS, production Singapore. Bản đọc nhanh cho người mới: [docs/](../../docs/README.md) (system design, database design, features). Lịch sử: [log](../log.md). Prototype ban đầu: https://claude.ai/artifact/6f6FHzuvBHg8L9A5SY71Zn

## Những gì đã biết
- Dự án mới, làm từ đầu. EM đóng vai Engineering Manager; agent đóng vai đội fullstack.
- Bảo mật là yêu cầu bắt buộc từ ngày đầu: authN/authZ phía server, chống truy cập/đánh cắp nội dung (đặc biệt là video) qua DevTools/JS.
- **Đa spa (multi-tenant) từ đầu**: dữ liệu tách theo spa + chi nhánh; launch cho 1 spa (EM chọn 2026-11-04).
- Stack đã chốt: [ADR-0001](decisions/ADR-0001-tech-stack.md); kiến trúc: [system](architecture/system.md).

## Câu hỏi mở (chờ EM)
Mỗi spec giữ câu hỏi của mình ở cuối trang; đây là những việc còn chờ quyết, gom một chỗ:
- **Production**: vùng và nơi chạy API (Vercel / Fly.io / VPS Singapore), Vercel Pro khi dùng thật: [ADR-0012](decisions/ADR-0012-colocate-api-db-singapore.md) (Q-P2, Q-P3; staging ở lại Mỹ).
- **Admin nền tảng**: Q-PA1…4 ở [platform-admin](product/specs/platform-admin.md) §cuối; [ADR-0018](decisions/ADR-0018-platform-admin-console.md) đã làm, EM chưa duyệt.
- **Quy tắc lương**: 8 ⚠️ mâu thuẫn ở [compensation-rules](product/compensation-rules.md) (đã có mặc định chỉnh được, cần chốt trước R2); ADR-0010 (hoa hồng khi hoàn gói) chờ EM xác nhận.
- **ADR do lead quyết, EM chưa duyệt**: [ADR-0008](decisions/ADR-0008-payroll-close-snapshots.md), [ADR-0010](decisions/ADR-0010-commission-follows-kept-money.md), [ADR-0013](decisions/ADR-0013-employment-periods.md), [ADR-0014](decisions/ADR-0014-former-staff-login.md) (phần thiết kế), ADR-0018.
- **Trước khi dùng dữ liệu thật**: thủ tục chuyển dữ liệu ra nước ngoài (Luật 91/2025/QH15, [ADR-0002](decisions/ADR-0002-database-hosting.md)), Supabase Pro có backup, việc ở cuối [vercel-staging](runbooks/vercel-staging.md).
