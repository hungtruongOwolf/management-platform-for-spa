---
title: Project overview
updated: 2026-11-09
sources: [raw/2026-11-04-kickoff-request.md, raw/2026-11-04-idea-internal-platform.md]
---

# Viên Phúc Y Đường — Overview

**Giai đoạn hiện tại (2026-11-09):** build + qa liên tục trên staging (Vercel `pdx1` + Supabase `us-west-2`). Đã build: R1-1 quầy + gói liệu trình, R1-2 màn quản lý + báo cáo, R1-3 lịch hẹn, quản lý nhân sự / chấm công / đánh giá / lương, tài khoản + mật khẩu (ADR-0016), rà soát DB 2026-11 (0033–0037), console nền tảng (ADR-0018, 0038–0040). Chưa có: sổ thu chi, công nợ, nhắc Zalo/SMS, production Singapore. Bản đọc nhanh cho người mới: [docs/](../../docs/README.md) (system design, database design, features). Lịch sử: [log](../log.md). Prototype ban đầu: https://claude.ai/artifact/6f6FHzuvBHg8L9A5SY71Zn

## Những gì đã biết
- Dự án mới, làm từ đầu. EM đóng vai Engineering Manager; agent đóng vai đội fullstack.
- Bảo mật là yêu cầu bắt buộc từ ngày đầu: authN/authZ phía server, chống truy cập/đánh cắp nội dung (đặc biệt là video) qua DevTools/JS.
- **Đa spa (multi-tenant) từ đầu**: dữ liệu tách theo spa + chi nhánh; launch cho 1 spa (EM chọn 2026-11-04).
- Stack đã chốt: [ADR-0001](decisions/ADR-0001-tech-stack.md); kiến trúc: [system](architecture/system.md).

## Câu hỏi mở
- Q-M2 (2026-11-08, F4): **đã quyết** — mỗi hóa đơn gói một ghi chú mã hóa (`sale.note_ct`, 0030), xem [front-desk §10.15](product/specs/front-desk-and-packages.md). Câu hỏi còn mở nằm cuối mỗi spec (Q-PA1…4 ở [platform-admin](product/specs/platform-admin.md)).
- 8 ⚠️ quy tắc lương: [compensation-rules](product/compensation-rules.md) (cần trước R2).
