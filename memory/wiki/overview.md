---
title: Project overview
updated: 2026-11-04
sources: [raw/2026-11-04-kickoff-request.md, raw/2026-11-04-idea-internal-platform.md]
---

# Viên Phúc Y Đường — Overview

**Giai đoạn hiện tại:** idea ✓ · spec R1-1 ✓ · architect ✓ · backend/frontend design ✓ · breakdown ✓ (2026-11-04) → tiếp theo `/build S0`. Plan: [r1-1-front-desk](plans/r1-1-front-desk.md). Prototype: https://claude.ai/artifact/6f6FHzuvBHg8L9A5SY71Zn

## Những gì đã biết
- Dự án mới, làm từ đầu. EM đóng vai Engineering Manager; agent đóng vai đội fullstack.
- Bảo mật là yêu cầu bắt buộc từ ngày đầu: authN/authZ phía server, chống truy cập/đánh cắp nội dung (đặc biệt là video) qua DevTools/JS.
- **Đa spa (multi-tenant) từ đầu**: dữ liệu tách theo spa + chi nhánh; launch cho 1 spa (EM chọn 2026-11-04).
- Stack đã chốt: [ADR-0001](decisions/ADR-0001-tech-stack.md); kiến trúc: [system](architecture/system.md).

## Câu hỏi mở
- Q-M2 (2026-11-08, F4): giỏ chỉ có gói có cần ô ghi chú không, ai đọc ở đâu? Xem [front-desk §10.14](product/specs/front-desk-and-packages.md).
- 8 ⚠️ quy tắc lương: [compensation-rules](product/compensation-rules.md) (cần trước R2).
