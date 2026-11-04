---
title: ADR-0003 Multi-tenancy model
status: accepted
date: 2026-11-04
deciders: EM, agent
---

# ADR-0003: Multi-tenancy (đa spa)

## Context
EM chọn hướng SaaS: platform về sau có thể phục vụ nhiều spa; launch với 1 spa. Tải mỗi spa rất nhỏ. Rò rỉ dữ liệu giữa các spa là sự cố nghiêm trọng nhất có thể xảy ra.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **Chung DB, chung schema, cột `tenant_id` + RLS** | Rẻ nhất; migrate một lần cho mọi spa; báo cáo nền tảng dễ | Cách ly phụ thuộc vào kỷ luật code, cần RLS làm lớp thứ hai |
| Mỗi spa một schema | Cách ly tốt hơn | Migration nhân theo số spa; pool kết nối phức tạp |
| Mỗi spa một database | Cách ly mạnh nhất | Đắt; vận hành nặng; quá mức cần cho quy mô này |

## Decision
**Chung schema** (`app`) với `tenant_id NOT NULL` trên mọi bảng nghiệp vụ, `branch_id` trên bảng gắn với địa điểm. Hai lớp bảo vệ:
1. **Ứng dụng:** `tenant_id` lấy từ session (không bao giờ từ request). Mọi repository function nhận `TenantId` (newtype) làm tham số bắt buộc, nên quên truyền thì không compile được. Mọi query có điều kiện `tenant_id = $n`.
2. **Database:** RLS bật trên mọi bảng, policy `tenant_id = current_setting('app.tenant_id')::uuid`. API chạy `SET LOCAL app.tenant_id` đầu mỗi transaction, bằng role `app_rw` (không phải owner bảng, không có `BYPASSRLS`).
- Khóa ngoại tham chiếu kèm tenant: `(tenant_id, id)` nên không thể trỏ sang bản ghi của spa khác.
- Platform admin dùng endpoint riêng, permission riêng; mọi lần truy cập dữ liệu của spa đều ghi audit.

## Consequences
- Positive: chi phí gần 0 cho mỗi spa mới; một lần migrate.
- Negative: mọi query phải nhớ tenant. Rủi ro này được giảm bằng newtype bắt buộc, RLS và **test cách ly bắt buộc** (spa A đọc ID của spa B → 404) cho mọi endpoint.
- **Platform admin access (EM chốt 2026-11-04):** mặc định **không** đọc được dữ liệu khách, sức khỏe, tài chính, lương của spa. Khi cần hỗ trợ, chủ spa cấp **support grant** có thời hạn (mặc định 24 giờ, thu hồi được). Mọi truy cập trong thời gian grant đều được audit và chủ spa xem được. Không cần grant cho: tạo spa, cấu hình kỹ thuật, xem metadata vận hành (số user, dung lượng, lỗi).

## Revisit when
Có spa lớn yêu cầu dữ liệu tách hẳn (hợp đồng/pháp lý) → chuyển spa đó sang DB riêng.
