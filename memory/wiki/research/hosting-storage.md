---
title: Hosting & storage research — DB, files, video, compute
updated: 2026-11-04
sources: [raw/2026-11-04-kickoff-request.md]
---

# Lưu trữ & hosting dài hạn

> **Phạm vi hiện tại (EM, 2026-11-04):** chưa có video, chỉ có **dữ liệu khách hàng**. Giai đoạn này chỉ cần **Postgres (Supabase) + backup ra R2**. Phần video bên dưới để dành cho sau.
>
> **Hiện trạng (2026-11-09):** compute không theo đề xuất VPS ở cuối trang. Staging chạy **Vercel (web tĩnh + API container) + Supabase Mỹ** ([ADR-0005](../decisions/ADR-0005-compute-hosting.md) update 2026-11-05, [vercel-staging](../runbooks/vercel-staging.md)); production đang chờ [ADR-0012](../decisions/ADR-0012-colocate-api-db-singapore.md). Backup R2 chưa dựng (staging chỉ có dữ liệu giả).

Giá lấy từ các bài tổng hợp của bên thứ ba, cập nhật trong khoảng 06–10/2026. Trước khi trả tiền phải kiểm tra lại trên trang giá chính thức.

## Nguyên tắc: tách 3 loại dữ liệu, mỗi loại lưu một nơi rẻ nhất cho nó
| Loại | Đặc điểm | Nơi lưu |
|---|---|---|
| Dữ liệu nghiệp vụ (user, đơn, lịch, quyền…) | Dung lượng nhỏ (vài GB, nhiều năm mới lên chục GB), cần transaction và độ an toàn cao | **Managed PostgreSQL** |
| File / ảnh / tài liệu / backup | Dung lượng vừa đến lớn | **Object storage không phí egress**: Cloudflare R2 |
| Video | Dung lượng lớn; **băng thông xem là chi phí chính** | Dịch vụ video/CDN rẻ (Bunny Stream) hoặc R2 + tự đóng gói HLS |

Google Drive không dùng làm backend: dính giới hạn quota API, không có CDN, không có SLA, vi phạm điều khoản khi dùng để phát nội dung công khai, và tài khoản có thể bị khóa.

## Database: Supabase (deal YC: $300 credits / 12 tháng)
- Gói Pro: $25/tháng, gồm 8 GB DB, 250 GB egress, 100k MAU. Kèm $10 compute credit, vừa đủ 1 instance Micro.
  - Phí vượt mức: DB $0.125/GB, egress $0.09/GB.
- **$300 / 12 tháng ≈ $25/tháng**, vừa khít gói Pro. Nếu không vượt quota thì miễn phí khoảng 12 tháng.
  - Lên compute Small: credit hết sau khoảng 10 tháng.
  - Dùng nhiều egress: hết nhanh hơn nữa.
- Region đặt **Singapore** để latency thấp khi truy cập từ VN.
- Supabase là Postgres thuần, nên Rust/SQLx kết nối trực tiếp. Muốn rời đi thì chỉ cần `pg_dump`, mức lock-in thấp.
- **Bảo mật bắt buộc:** Supabase tự expose schema `public` qua Data API (PostgREST) bằng `anon` key. Rust API là trust boundary duy nhất, nên phải:
  - Tắt Data API, hoặc chỉ expose một schema rỗng.
  - Đặt bảng ở schema riêng.
  - Bật RLS trên mọi bảng làm lớp phòng thủ thứ hai.
  - Không đưa `service_role` key ra client.
- Supabase Storage **không dùng cho video**: egress $0.09/GB. Ví dụ 5 TB/tháng tốn khoảng $430.
- Alternative: Neon (serverless Postgres, scale-to-zero, rẻ cho staging/preview), hoặc tự host Postgres trên VPS (rẻ nhất nhưng tự lo backup, HA, patch).

## File & backup: Cloudflare R2
- $0.015/GB-tháng, **egress $0**. Free 10 GB.
  - 1 TB chỉ tốn khoảng $15/tháng.
  - Gói Infrequent Access $0.01/GB, hợp cho backup.
- Backup: dump Postgres hằng đêm lên bucket R2 riêng, bật object lock/versioning. Đây là lớp backup độc lập với backup của Supabase.

## Video
- **Bunny Stream:**
  - Storage khoảng $0.01/GB.
  - Delivery Asia khoảng $0.03/GB (EU/NA $0.01).
  - DRM MediaCage Enterprise: $99/tháng + $0.005/licence.
  - Giai đoạn đầu chạy không DRM; bật DRM khi bắt đầu bán khóa học.
- **R2 + HLS tự đóng gói** (ffmpeg, AES-128, signed URL qua Worker): rẻ nhất vì egress $0, nhưng không có DRM thật. Xem `.claude/skills/secure/content-protection.md`.
- Ước tính cho 1 TB video và 5 TB lượt xem mỗi tháng:
  - Bunny: khoảng $10 storage + $150 delivery.
  - R2: khoảng $15 tổng.
  - Supabase Storage: khoảng $430 cho riêng egress.

## Compute (Rust API + Next.js)
- Rust ăn rất ít RAM, nên một VPS 2 vCPU / 4 GB chạy được cả API lẫn Next.js trong giai đoạn đầu.
- Hetzner đã tăng giá 2 lần trong 2026 (04 và 06), và capacity ở Singapore hạn chế. Các lựa chọn khác: DigitalOcean / Vultr / Linode Singapore, Fly.io, hoặc cloud trong nước (Viettel IDC, VNPT, FPT, BizFly) `(giá chưa xác minh)`.

## Pháp lý: dữ liệu ra nước ngoài
- Luật Bảo vệ dữ liệu cá nhân **91/2025/QH15** và **Nghị định 356/2025/NĐ-CP** có hiệu lực từ 01/01/2026, thay Nghị định 13/2023.
- Lưu trữ hay xử lý trên cloud ở nước ngoài (kể cả Singapore) đều tính là **chuyển dữ liệu xuyên biên giới**. Phải lập và nộp **hồ sơ đánh giá tác động chuyển dữ liệu (TIA)** trong 60 ngày, qua cổng của Bộ Công an.
- Hồ sơ/dữ liệu sức khỏe là dữ liệu **nhạy cảm**. Mức phạt vi phạm chuyển dữ liệu xuyên biên giới có thể lên tới 5% doanh thu năm trước.
- → Nếu sản phẩm lưu hồ sơ bệnh án hay dữ liệu sức khỏe: cân nhắc đặt DB ở cloud trong nước, hoặc làm TIA ngay từ đầu. **Cần hỏi luật sư trước khi launch.**

## Đề xuất (chờ `/architect` chốt thành ADR)
- **Năm 1:**
  - DB: Supabase Pro, Singapore, dùng credits.
  - File và backup: R2.
  - Video: Bunny Stream.
  - API + web: 1 VPS nhỏ.
  - Chi phí ngoài credits: khoảng $10–30/tháng, cộng thêm phần băng thông video.
- **Năm 2+:**
  - Ở lại Supabase nếu chi phí hợp lý.
  - Nếu DB lớn hoặc cần dữ liệu ở trong nước: chuyển sang Postgres tự quản. Do là Postgres thuần nên `pg_dump`/restore là xong.
