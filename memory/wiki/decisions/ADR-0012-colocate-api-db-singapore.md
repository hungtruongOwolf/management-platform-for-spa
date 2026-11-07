---
title: ADR-0012 Đặt API và DB cùng ở Singapore, API luôn chạy
status: proposed
date: 2026-11-07
deciders: EM, agent
---

# ADR-0012: Đặt API + DB cùng ở Singapore, API luôn chạy (không scale to zero)

## Context
EM báo API và UI chậm ([raw](../../raw/2026-11-07-em-latency-request.md)). Số đo ngày 2026-11-07 nằm ở [performance-latency-2026-11](../architecture/performance-latency-2026-11.md):
- Staging chạy ở Oregon: Vercel container `pdx1` + Supabase `us-west-2` ([ADR-0005 update 2026-11-05](ADR-0005-compute-hosting.md)). Người dùng ở Việt Nam.
- RTT từ VN: tới Oregon 179–226 ms, tới Singapore 35–51 ms. `GET /health` từ VN có TTFB 193–746 ms, trong khi Postgres chỉ chạy query 0,25–1,7 ms mỗi request.
- Mỗi request chạy 8–17 câu SQL nối tiếp. Vì vậy API và DB phải luôn ở cùng một vùng.
- Container Vercel cold start mất 1,3–1,8 s. Hiện tượng này xuất hiện sau chỉ ~45 s không có request, và không có tùy chọn min instance. Spa có 3–8 người và nghỉ giữa các khách, nên lần bấm đầu sau mỗi khoảng nghỉ thường gặp cold start.
- Vercel Hobby không cho dùng thương mại.

Thứ tự thuộc tính chất lượng ([system](../architecture/system.md)): bảo mật > hiệu năng ở quầy (tìm khách < 300 ms, lưu lượt < 500 ms p95) > chi phí > vận hành đơn giản.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| Giữ Oregon, chỉ tối ưu code | $0, không đổi vendor | Mỗi request vẫn tốn 200–450 ms từ VN, vẫn cold start. Không đạt ngân sách tìm khách < 300 ms p95. |
| A. Vercel container `sin1` + Supabase `ap-southeast-1` | Ít việc nhất (sửa `regions`, tạo DB mới) | Vẫn cold start. Beta: ⚠ chưa xác nhận `sin1` có hỗ trợ container. Dùng thật phải lên Pro $20. |
| **B. SPA trên Vercel + API Fly.io `sin` (`min_machines_running = 1`) + Supabase `ap-southeast-1`** | Luôn chạy nên hết cold start, pool DB sống lâu. Vẫn một origin qua Vercel rewrite nên cookie `__Host-`, CSRF origin check và CSP giữ nguyên. ~$5–9/tháng. | Thêm vendor. Phải khóa origin Fly bằng header bí mật. Đi qua edge `hkg1` cộng ~30–40 ms. Vercel Pro vẫn cần khi dùng thật. |
| C. VPS Singapore (Caddy + API, có thể phục vụ cả SPA) + Supabase `ap-southeast-1` | Đúng quyết định production gốc của ADR-0005. Rẻ ($6–12). Kiểm soát log/header, nên giải được L5 (query string trong log). | Tự vá OS, tự theo dõi. Một điểm lỗi duy nhất. |

## Decision (proposed)
Đặt **API và DB cùng ở Singapore**, và **API luôn chạy**:
- Supabase project mới ở `ap-southeast-1`, Supavisor session mode như hiện nay.
- **Staging:** phương án **B** (Fly.io `sin`, một machine luôn chạy, SPA vẫn ở Vercel, Vercel rewrite `/api/*` sang Fly).
- **Production:** chọn giữa B và C khi spa dùng thật. C vẫn là mặc định của ADR-0005 nếu EM chấp nhận tự vận hành một VPS.
- Nếu EM muốn chỉ sửa cấu hình: A là bước trung gian chấp nhận được. Nó bỏ được ~150–350 ms mỗi request nhưng không bỏ được cold start.

**Chờ EM quyết:** Q-P1 (vùng), Q-P2 (A/B/C), Q-P3 (Vercel Pro khi dùng thật).

## Consequences
- Positive:
  - API p50 từ VN dự kiến ~70–110 ms (hiện ~250–450 ms).
  - Một màn hình dự kiến ~0,2–0,35 s (hiện 0,6–1,5 s).
  - Không còn cold start 1,3–1,8 s (với B/C).
  - DB ở Singapore đúng như ADR-0002.
- Negative / accepted trade-offs:
  - Phải tạo lại DB staging (dữ liệu demo tạo lại được) và có URL/mật khẩu mới.
  - B thêm một vendor, một bí mật (header origin) và một bước deploy (`fly deploy`).
  - Vercel rewrite tôn trọng cache upstream, nên API **phải** gửi `Cache-Control: private, no-store` (hoặc `private, no-cache` + ETag) trên mọi response.
- Follow-ups:
  - Thêm layer `Cache-Control` mặc định cho API (secure).
  - Cập nhật runbook [vercel-staging](../runbooks/vercel-staging.md) và CSP/`APP_PUBLIC_ORIGIN` nếu cần.
  - Chạy lại script đo + Globalping và ghi số trước/sau.
  - Job purge idempotency chạy theo lịch thay vì chỉ lúc khởi động, vì server sống lâu.
  - Rate limit trong bộ nhớ chính xác hơn khi chỉ có một instance.

## Revisit when
- Số đo sau khi dời vẫn > 150 ms p50 từ VN.
- Có nhiều spa / nhiều chi nhánh (cần 2 instance + load balancer).
- Luật hoặc tư vấn pháp lý yêu cầu dữ liệu nằm ở Việt Nam.
