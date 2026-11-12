---
title: ADR-0024 Màn đầu là chunk riêng, tải song song với startup; gate theo từng đường
status: accepted
date: 2026-11-12
deciders: agent perf (EM ủy quyền "làm hết đi cho tôi, tối ưu", không hỏi lại)
---

# ADR-0024: Màn đầu (`/quay`, trang chủ) là chunk riêng được tải song song với bundle khởi động; ngân sách đo cả startup chung lẫn đường riêng của từng màn

> Thay quy tắc cũ ở [frontend § Perf budget](../architecture/frontend.md#perf-budget-critical-path): "bundle chính chứa shell, `/dang-nhap` **và `/quay`**" (2026-11-05). Số đo và quy tắc giữ startup gọn: ở trang đó, mục "Headroom 2026-11-12".

## Context
- JS khởi động 124,90 / 125 KiB gzip -9 (2026-11-12): không còn chỗ cho tính năng kế tiếp. EM: tạo headroom thật, không nâng ngân sách, không bỏ tính năng, không đổi hành vi; mục tiêu ≤ 115 KiB.
- Đo bằng sourcemap: thư viện ~100 KiB (react-dom 61, TanStack Router 24,5, Query 9,7), mã app ~21 KiB, trong đó màn quầy ~8,6 KiB. Không có cách cắt ≥ 10 KiB mà vẫn để `/quay` trong bundle khởi động.
- `/quay` ở trong bundle chính vì một chunk lazy thường được tải **sau** khi bundle chính chạy: thêm một lượt đi về trước LCP của màn mở cả ngày. CSP `script-src 'self'` cấm script inline. Phiên hết hạn sau 12 h nghỉ (ADR-0004): sáng nào trang đăng nhập cũng là màn đầu, mở từ bất kỳ đường dẫn nào.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. Giữ `/quay` trong startup, cắt lẻ | Không đổi cấu trúc | Tối đa ~3 KiB, không tới 115 |
| B. `/quay` lazy thường | Startup −10 KiB | `/quay` thêm một lượt đi về trước LCP (~150–300 ms mobile) |
| C. `/quay`, `/` lazy + script nhỏ (file, build sinh) đọc URL và `modulepreload` chunk của màn cùng lúc với startup; gate đo startup **và** từng đường | Startup −11 KiB cho mọi màn; `/quay` không thêm lượt đi về; trang chủ bớt một lượt (chunk home trước đây nối tiếp) | Một request nhỏ thêm (0,35 KiB, tính vào startup); tách chunk tốn ~1–2 KiB trên đường `/quay` → phải cắt thật ở quầy để đường đó không tăng |
| D. HTML riêng cho `/quay` (rewrite Vercel / Caddy) | Không cần script | Cấu hình deploy hai nơi, vẫn tốn chi phí tách chunk như C |
| E. `LoginPage` cũng lazy | Thêm −1 KiB | Sáng nào cũng thêm một lượt đi về trước form đăng nhập (chuyển hướng sau `/me` 401) |

## Decision
Chọn **C**. `app/screens.ts` giữ loader của màn đầu lazy (router, `AppShell`, `LoginPage` dùng chung); `ROUTE_PRELOAD` (`vite.config.ts`) + plugin `scripts/route-preload.mjs` sinh script preload; `scripts/startup-budget.mjs` đọc lại bảng và gate mỗi đường ở cùng 125 KiB. Để đường `/quay` giảm thật chứ không chỉ chuyển chỗ: phần giỏ (tổng tiền, Lưu, việc của ba lần lưu, JSX giỏ) thành `CheckoutPanel` lazy sau lần vẽ đầu; state và `useMutation` ở lại `FrontDeskPage`. `LoginPage` ở lại startup (không chọn E); `lib/errors.ts` rời startup.

## Consequences
- Positive: startup chung 124,90 → 113,67 KiB (~11 KiB headroom cho shell và mọi màn khác); `/quay` 124,90 → 123,86 (giảm thật); `/` 122,16 tải song song; Lighthouse vẫn đo `/quay` và `/dang-nhap` trên bản build.
- Negative / accepted trade-offs: headroom của riêng đường `/quay` chỉ ~1,1 KiB; mã mới của màn đầu quầy phải vào chunk lazy (sau lần vẽ đầu). Một file script nhỏ thêm vào mỗi lần mở app. Dev server không chạy script (màn lazy tải như các màn khác). Nhóm `codeSplitting` (`FrontDeskPage`, `common`) phải sửa khi đổi import của các module trong đó.
- Follow-ups: theo dõi Lighthouse CI `/quay` (trước 2112 ms trung vị); nếu thêm màn đầu khác (ví dụ `/nhan-vien` của KTV) thì thêm vào `ROUTE_PRELOAD` + `app/screens.ts`.

## Revisit when
Đường `/quay` chạm 125 KiB, hoặc có service worker / HTTP 103 Early Hints cho phép preload theo đường dẫn ở tầng mạng, hoặc đổi router.
