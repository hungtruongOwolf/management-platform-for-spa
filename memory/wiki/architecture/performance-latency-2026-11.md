---
title: Performance & latency — đo staging và phương án kiến trúc (2026-11)
updated: 2026-11-07
sources: [raw/2026-11-07-em-latency-request.md, decisions/ADR-0005-compute-hosting.md, runbooks/vercel-staging.md, architecture/frontend.md, architecture/backend.md]
---

# Performance & latency: đo staging, nguyên nhân, phương án (2026-11-07)

EM thấy "API và UI đang rất chậm" ([raw](../../raw/2026-11-07-em-latency-request.md)). Trang này ghi số đo thật trên staging, nguyên nhân xếp theo mức ảnh hưởng, các phương án có chi phí, và kế hoạch theo giai đoạn. Đợt này **chỉ đo và nghiên cứu**: không đổi code, không đổi cấu hình Vercel/Supabase.

**Kết luận ngắn:** gần như toàn bộ độ trễ là **khoảng cách mạng**. API và DB đặt ở Oregon (Vercel `pdx1` + Supabase `us-west-2`), còn người dùng ở Việt Nam. Thời gian DB chạy query **dưới 2 ms** mỗi request, thời gian server khoảng 25–50 ms, còn mỗi lượt đi về giữa Việt Nam và Oregon mất **~200–450 ms**. Một màn hình cần 2–4 lượt nối tiếp nhau, nên thành 0,6–1,5 s. Chỉ cần nghỉ **~45 giây** là lần gọi đầu tiếp theo tốn thêm **~1,2–1,7 s** (cold start của container Vercel). Cách sửa lớn nhất là **dời cả API lẫn DB sang Singapore cùng lúc**. Có thêm vài việc engineering làm ngay được, không tốn tiền.

## 1. Cách đo
- **Nơi đo:** máy dev của đội. IP ra Internet của máy này nằm ở **Texas, Mỹ** (`ipinfo.io`; edge Vercel `cle1`). Log request của Vercel trong 3 giờ gần nhất cũng chỉ có client ở Mỹ: trình duyệt Windows qua `cle1` (nhiều khả năng là EM đang dùng VPN hoặc mạng ra Mỹ), Linux Chrome qua `sfo1`. Vì vậy số đo đã đăng nhập ở §2 là **từ Mỹ**: chúng **lạc quan hơn** những gì spa ở Việt Nam sẽ gặp.
- **Đo từ Việt Nam thật:** dùng [Globalping](https://globalping.io) (probe ở TP.HCM, Hà Nội, Thanh Hóa; mạng VNPT, CMC, Saigon Postel, VNG, Viet Storage, 365 Online, Verichains). Chỉ gọi endpoint công khai (`/api/v1/health`, `/api/v1/me` không cookie → 401, file tĩnh). Không bao giờ gửi cookie cho bên thứ ba.
- **Đã đăng nhập:** tài khoản quản lý demo `0905000002`, một kết nối HTTP/2 giữ sống như trình duyệt, 12 mẫu tuần tự cho mỗi endpoint, cách nhau 230 ms để không chạm rate limit 5 rps. Mật khẩu đọc từ `.env.staging` trong script, không in ra, không qua argv. Đăng xuất sau khi đo.
- **Phía DB:** owner URL (chỉ đọc, `BEGIN READ ONLY`). Đọc `pg_stat_statements` trước và sau 5 lần gọi mỗi endpoint, lấy hiệu số để biết **số câu lệnh SQL mỗi request** và thời gian chạy thật trong Postgres.
- Script đo: `infra/perf/` (từ 2026-11-07: `endpoints.mjs` = §2b + đọc `Server-Timing` thay cho hiệu số `pg_stat_statements`, `gaps.mjs` = §2e, `globalping.mjs` = §2a; mật khẩu đọc từ `.env.staging` lúc chạy, xem README ở đó).

## 2. Số đo

### 2a. Từ Việt Nam (Globalping, request không cookie)
| Đích | Kết quả (ms) | Ghi chú |
|---|---|---|
| TCP RTT VN → AWS Singapore (`ap-southeast-1`) | **35–51** | 5 probe |
| TCP RTT VN → AWS Hong Kong (`ap-east-1`) | 26–45 | |
| TCP RTT VN → AWS Oregon (`us-west-2`, chỗ DB staging) | **179–226** | |
| HTML / JS tĩnh, cache HIT ở edge `hkg1`/`sin1` | TTFB **38–72** | cache MISS (lần đầu mỗi PoP sau mỗi deploy) 290–570 |
| `GET /api/v1/health` (không DB), `hkg1 → pdx1` | TTFB **193–746**, trung vị ~220 (mạng DC) và ~300–450 (VNPT, Saigon Postel, Viet Storage) | `x-vercel-id: hkg1::pdx1::…`; tính sau khi đã bắt tay TLS |
| `GET /api/v1/me` không cookie (401: một lần tra session) | TTFB 199–424 | |

So sánh: từ Texas cùng endpoint `health` chỉ mất ~125 ms. Request từ Việt Nam vào edge Hong Kong rất nhanh (~40 ms), rồi **băng qua Thái Bình Dương tới `pdx1`** cho mọi request API.

### 2b. Đã đăng nhập, từ Mỹ (HTTP/2 giữ kết nối, n = 12)
"SQL/req" = số câu lệnh mỗi request, đo bằng hiệu số `pg_stat_statements`. "DB exec" = tổng thời gian Postgres chạy các câu đó.

| Endpoint (màn) | p50 | p95 | SQL/req | DB exec | JSON → wire |
|---|---|---|---|---|---|
| `GET /health` (mốc: chỉ mạng + edge) | ~125 | ~140 | 0 | 0 | 33 B |
| `GET /me` (mọi màn) | 158 | 185 | 8 | 0,54 ms | 0,7 KB |
| `GET /visits?date=` (Quầy) | 166 | 196 | 14 | 0,72 ms | 2,7 → 1,0 KB br |
| `GET /customers?q=nguyen` (tìm khách) | 158 | 515\* | 10 | 0,27 ms | 0,8 KB |
| `GET /customers?q=0399` | 153 | 184 | 10 | | 2,6 → 0,8 KB br |
| `GET /customers` (gần đây) | 159 | 190 | | | 2,6 → 0,9 KB br |
| `GET /services` | 165 | 173 | 11 | 0,40 ms | 8,6 → 2,1 KB br |
| `GET /service-categories` | 153 | 179 | | | 0,9 KB |
| `GET /package-plans` | 159 | 267 | | | 1,5 → 0,6 KB br |
| `GET /staff` | 157 | 181 | 10 | 0,25 ms | 0,7 KB |
| `GET /appointments` 1 ngày (Lịch hẹn) | 171 | 194 | 17 | 1,15 ms | 2,8 → 1,2 KB br |
| `GET /appointments` 1 tuần | 171 | 220 | | | 20,4 → 3,8 KB br |
| `GET /reports/daily` (Tổng quan) | 172 | 217 | 13 | 1,08 ms | 1,0 → 0,4 KB br |
| `GET /reports/range?preset=last_30_days` | 173 | 187 | 16 | 1,70 ms | 20,8 → 2,7 KB br |
| `GET /reports/ledger` tháng này | 178 | 228 | | | 10,0 → 2,4 KB br |
| `GET /attendance?month=` (Nhân viên) | 173 | 241 | 17 | 0,48 ms | 2,3 → 0,5 KB br |
| `GET /daily-reviews?month=` | 165 | 244 | | | 1,1 → 0,4 KB br |
| `GET /me/staff?month=` | 149 | 188 | | | 16 B |
| `GET /payroll`, `/payroll/payslips/{id}` | 143 / 141 | 225 / 158 | 7 | | 403: quản lý không có quyền lương. Chủ spa cần TOTP nên không đo được bằng script; theo số SQL/req thì cùng cỡ `/attendance`. |
| `POST /auth/login` | 204–252 (ấm) | 1371–1446 (lần đầu) | | | lần đầu = cold start, xem 2c |

\* Một mẫu 515 ms là jitter mạng: cùng query chạy 0,27 ms trong DB.

**Tách thời gian một request API (ấm, từ Mỹ):** ~125 ms là mạng client → edge → `pdx1`. Phần còn lại, **25–50 ms**, là server: 8–17 câu SQL chạy **nối tiếp** qua Supavisor, mỗi câu ~3 ms đi về, cộng phần Rust. Riêng thời gian Postgres chạy query chỉ **0,25–1,7 ms**.

**Ước tính từ Việt Nam** = mốc `health` đo từ VN + phần server: **p50 ~250 ms** trên mạng tốt, **~350–500 ms** trên mạng gia đình, p95 600–800 ms.

### 2c. Cold start (scale to zero)
- Request đầu sau khi nghỉ ~7 phút: **1 772 ms**, các request ngay sau: 220–236 ms (kết nối mới mỗi lần). Request đầu của mỗi lần chạy script (login, health) cũng mất **1,3–1,5 s**, lần sau ~200 ms.
- `pg_stat_statements` từ 2026-11-04 có **91 lần** câu kiểm tra role lúc khởi động (`check_runtime_role`, chạy một lần mỗi instance) và **190 lần** `DISCARD ALL` (Supavisor dọn kết nối khi client đóng). Nghĩa là trong 3 ngày có ~90 instance được dựng (cả deploy) và kết nối DB bị mở/đóng liên tục.
- Mỗi cold start gồm: boot container, mở pool (TLS tới pooler + auth), kiểm tra role, job purge idempotency. Spa chỉ có vài người và nghỉ giữa các khách, nên **lần bấm đầu sau mỗi khoảng nghỉ** sẽ hay gặp cold start.
- Đo theo khoảng nghỉ (`gaps.mjs`): xem §2e. Lần gọi đầu chậm xuất hiện sau chỉ ~45 s nghỉ.

### 2d. UI (code + Lighthouse CI)
- **Bundle:** entry `index-*.js` 132 KB br (gzip -9 ~124,8 KB, ngân sách 125), CSS 19,6 KB br. Màn quản lý tách chunk lazy. `/me` được preload song song với JS (`index.html` `<link rel=preload>`). Asset `immutable` 1 năm, HTML `no-cache`, edge HIT. **Bundle không phải nút thắt.**
- **Lighthouse CI** (run 42361491947, main 2026-11-07, localhost + Slow 4G mô phỏng): `/quay` LCP 2201 / 2362 / **2717** ms (ngân sách ≤ 2500 ở median: đang sát mép), FCP ~1658, TBT 12–60, CLS 0,001, 37 request, 341 KB. `/dang-nhap` LCP ~2105. Lighthouse chạy với API ở localhost (p95 API trong CI 4–6 ms), nên **không thấy** độ trễ Thái Bình Dương.
- **Waterfall mỗi màn** (đọc code; L = số lượt đi về nối tiếp):

| Màn | Sâu nhất | API khi tải lạnh | Ghi chú |
|---|---|---|---|
| `/quay` (Ghi lượt) | L2 + sau lần vẽ đầu | 3 (`/me`, `/staff`, `/visits?limit=31`) | Chọn khách → `/customers/{id}` + `/packages` (staleTime 0) + catalog (5 phút) |
| `/quay?view=lich` | L3 | 4 | Chunk lịch nạp sau `/me`; vẫn gọi `/visits` dù không hiện; poll 60 s |
| `/` | L3 | 2 | |
| `/khach-hang/$id` | **L4** (chunk L5) | 6 | `packages`, `summary`, `history`, `follow-ups` chờ `/customers/{id}` xong mới mount, dù id đã có trong URL |
| `/bao-cao` | L3 | 3 | 2 range song song |
| `/nhan-vien` | L3 (L4 với `?day=`) | tới 6 | mọi query staff staleTime 0 |
| `/khach-cua-toi` | L3 | 2 | |

- **Refetch:** `QueryClient` chỉ đặt `retry:false`, nên `refetchOnWindowFocus` bật ở mọi nơi. Query không có staleTime (hồ sơ khách, gói, summary, history, mọi query nhân viên) **gọi lại mỗi lần mount và mỗi lần quay lại tab**. Ở VN, mỗi lần như vậy là 0,3–0,5 s.
- **Không có** prefetch (`defaultPreload`, `prefetchQuery`, hover). **Không có** optimistic update (`onMutate`). Lưu lượt **chờ** refetch gói (`await invalidate`) rồi mới hiện toast, nên thêm một lượt đi về sau mỗi lần lưu.
- Skeleton có đủ. `PageSkeleton` của route chỉ hiện sau ~1 s (mặc định `defaultPendingMs`).

### 2e. Khoảng nghỉ → lần gọi đầu chậm
`GET /health` từ Mỹ, hai request sau mỗi khoảng nghỉ. Khoảng nghỉ ≥ 5 s thì client mở kết nối TLS mới, nên mốc nền ~220 ms (không phải ~125).

| Nghỉ | Request đầu | Request ngay sau |
|---|---|---|
| 2 s | 124 | 125 |
| 5 / 10 / 20 / 30 s | 220 / 230 / 283 / 297 | 216–230 |
| **45 s** | **1 456** | 215 |
| **60 s** | **1 373** | 233 |
| **90 s** | **1 475** | 230 |
| **7 phút** | **1 772** | 228 |

Docs của Vercel nói container scale down sau 5 phút không có traffic. Nhưng thực đo, **chỉ cần nghỉ ~45 s** là lần gọi đầu đã tốn thêm ~1,2 s. Chưa tách được đây là instance khởi động lại hay đường edge → container dựng lại; với người dùng thì hai chuyện như nhau. Ở quầy, cứ vài phút mới có một thao tác, nên **phần lớn lần bấm đầu sau mỗi lượt khách sẽ chậm thêm ~1,2–1,5 s**. Đây rất có thể là cảm giác "rất chậm" EM nhắc tới.

## 3. Nguyên nhân, xếp theo ảnh hưởng
1. **Vùng: API + DB ở Oregon, người dùng ở Việt Nam.** Mỗi request API cộng ~180–400 ms mạng (2a). Đây là >80 % thời gian của mọi request. Không tối ưu code nào bù được.
2. **Lần gọi đầu sau khi nghỉ chậm thêm ~1,2–1,7 s** (container Vercel scale to zero / dựng lại). Thực đo, chỉ cần nghỉ ~45 s là đã gặp (§2e). Ở spa ít người, cứ vài phút mới có một thao tác, nên đây là tình huống **thường gặp nhất** chứ không phải hiếm. Theo cảm nhận người dùng thì nó còn đau hơn (1): một lần đứng 1,5–2 s rõ hơn nhiều so với 0,3 s mỗi request.
3. **Waterfall + refetch phía UI** nhân (1) lên: màn hình cần 2–4 lượt nối tiếp (`/me` → chunk → query → query con), và staleTime 0 + refetch-on-focus gọi lại khi quay lại tab. Hồ sơ khách là màn tệ nhất (L4).
4. **Nhiều câu SQL nối tiếp mỗi request** (8–17, trong đó ~8 là "ống nước": tra session, BEGIN, `set_config`, membership, quyền, COMMIT, rồi BEGIN/`set_config`/COMMIT lần hai). Hiện tại chỉ ~25–40 ms vì API nằm cạnh DB. Nhưng nếu **chỉ dời API** sang Singapore mà DB vẫn ở Oregon, thì 17 × ~170 ms ≈ **3 s mỗi request**. Hai thứ phải dời cùng nhau.
5. **Không phải nguyên nhân** (đã kiểm): thời gian chạy query (0,25–1,7 ms; bench 3 năm p95 ≤ 150 ms ở [backend §3b](backend.md)), N+1 (số câu lệnh cố định theo endpoint), kích thước response (≤ 21 KB JSON, br còn ≤ 4 KB), bundle JS, font, ảnh, nén (Vercel nén br cho response > ~1 KB), HTTP/2 (edge đã dùng h2).

**Phát hiện phụ (bảo mật, chuyển `secure`):** endpoint không tự đặt `Cache-Control` (`/customers?q=`, `/services`, `/staff`, `/payroll` 403…) bị Vercel gắn mặc định `public, max-age=0, must-revalidate`. Dữ liệu cá nhân (tên + SĐT khách) không nên mang `public`. Đề xuất một layer mặc định `Cache-Control: private, no-store` (`if_not_present`) cho mọi `/api`. **Đã làm (2026-11-07, `fix/api-cache-control-default`):** layer ngoài cùng trong router Axum, test `cache_control.rs`. `vercel.json` không có rule header nào cho `/api` (rule `no-cache` chỉ thuộc service `web`), nên không cần thêm.

## 4. Phương án

Nguồn tra ngày 2026-11-07. Trang nào không ghi ngày cập nhật thì đánh "n.d.". ⚠ là chỗ chưa kiểm chứng được.

### 4a. Vùng: dời API + DB sang Singapore (giải nguyên nhân 1)
- **Supabase** có `ap-southeast-1`, nhưng **không đổi vùng tại chỗ được**. Phải tạo project mới rồi chuyển dữ liệu (`pg_dump`/restore), và host, pooler, mật khẩu đều mới ([Supabase: change region](https://supabase.com/docs/guides/troubleshooting/change-project-region-eWJo5Z), 2025-03; [regions](https://supabase.com/docs/guides/platform/regions), n.d.). Staging chỉ có dữ liệu demo, nên chạy `app migrate` + bootstrap + `seed-demo` lại là xong. Production vốn đã định ở Singapore ([ADR-0002](../decisions/ADR-0002-database-hosting.md)).
- **Vercel** có `sin1` và `hkg1`. Hobby được 1 vùng tùy chọn, không giới hạn vùng nào ([function regions](https://vercel.com/docs/functions/configuring-functions/region), 2026-08; [regions](https://vercel.com/docs/regions), 2026-08). ⚠ Trang Container Runtime / Services (beta) không nói rõ `sin1` có được hỗ trợ không: cần một lần deploy thử.
- **Quy tắc:** API và DB **phải đổi vùng cùng lúc**. Mỗi request chạy 8–17 câu SQL nối tiếp, nên nếu API ở Singapore mà DB còn ở Oregon thì sẽ thành ~3 s mỗi request (§3.4).
- **Hiệu quả dự kiến từ VN:** edge ~40 ms + edge → origin ở SG 0–35 ms + server ~25 ms = **~70–110 ms mỗi request** (hiện ~250–450 ms). Mỗi màn hình (2–3 lượt nối tiếp) từ 0,6–1,5 s xuống **~0,2–0,35 s**.

### 4b. Nơi chạy API ở Singapore
| Phương án | Ưu | Nhược | Chi phí/tháng (USD) |
|---|---|---|---|
| **A. Vercel container `sin1`** (đổi `regions` trong `vercel.json`) | Ít việc nhất: một dòng cấu hình, giữ nguyên deploy, một origin | **Vẫn cold start** (scale to zero sau 5 phút theo [docs](https://vercel.com/docs/functions/runtimes/container), 2026-07; thực đo còn sớm hơn, §2e), không có min instance. Hobby **cấm dùng thương mại** ([fair use](https://vercel.com/docs/limits/fair-use-guidelines), 2026-09) nên spa dùng thật phải lên Pro. Beta. | Vercel Pro $20 (đã gồm $20 usage) + Supabase Pro $25 (đang trả bằng credit, ADR-0002) ≈ **$45**, thực chi ~$20 |
| **B. SPA ở Vercel + API trên Fly.io `sin`**, `min_machines_running = 1`, Vercel rewrite `/api/*` sang Fly | Luôn chạy nên **hết cold start**. Pool DB sống lâu. Deploy bằng Dockerfile có sẵn. Vẫn một origin nên cookie `__Host-` + CSP `'self'` giữ nguyên ([rewrites](https://vercel.com/docs/routing/rewrites), 2026-08). | Thêm một vendor. Rewrite qua `hkg1` cộng ~30–40 ms. Phải khóa origin Fly (header bí mật từ Vercel) và gửi `private, no-cache` vì project mới tôn trọng cache upstream. | Fly shared-cpu-1x 1 GB sin $8,50 (512 MB ⚠ ~$4,7) ([pricing](https://docs.fly.io/about/pricing), n.d.) + Vercel Pro $20 + Supabase $25 ≈ **$50–54** (thực chi ~$25–30) |
| **C. Một VPS Singapore** (DigitalOcean SGP1 / Lightsail SG) + Supabase SG: đúng quyết định production của [ADR-0005](../decisions/ADR-0005-compute-hosting.md) | Rẻ, luôn chạy, kiểm soát header, log (L5 query string) và job nền. Có thể phục vụ cả SPA (Caddy) để bỏ phụ thuộc Vercel. | Tự vá OS, theo dõi, TLS. Một điểm lỗi duy nhất (đã chấp nhận ở ADR-0005). | DO $6–12 ([droplets](https://www.digitalocean.com/pricing/droplets), ⚠ giá SGP1 chưa xác nhận) hoặc Lightsail $7 ([pricing](https://aws.amazon.com/lightsail/pricing/)) + Supabase $25 ≈ **$31–37** (thực chi ~$6–12); + $20 nếu SPA vẫn ở Vercel Pro |
| D. Một VPS chạy luôn Postgres | Rẻ nhất (~$15–20), RTT DB ~0 | Mất backup/PITR có quản lý, tự lo upgrade. Không đáng với dữ liệu tiền + sức khỏe. | Không khuyến nghị |
| Railway (Southeast Asia) / Render (Singapore) | PaaS tương tự B | Render free ngủ 15 phút, mất ~1 phút để dậy. Railway tính theo usage. | Railway Hobby $5 (+ usage) · Render Starter $7 |
| Cloud Run `asia-southeast1` min 1 instance | Managed | ⚠ Giá Tier 2 chưa xác nhận, ~$10+/tháng cho min instance | ~$10–60 |

**Fluid compute** của Vercel cho nhiều request dùng chung một instance, và Services/containers "billed the same as Functions… Fluid Compute applies" ([services pricing](https://vercel.com/docs/services/pricing), 2026-08). Axum vốn đã xử lý đồng thời trong một process. Phần giảm cold start của Fluid (bytecode caching) **chỉ áp dụng cho Node** ([fluid compute](https://vercel.com/docs/fluid-compute), 2026-08), nên **không giải được cold start của container Rust**.

### 4c. Pooler và prepared statements
- Đang dùng **Supavisor session mode (5432)**: mỗi client giữ một backend, prepared statement của SQLx chạy bình thường. Đây là lựa chọn đúng cho server sống lâu (B, C).
- Transaction mode (6543) **không hỗ trợ prepared statement** theo docs ([connecting](https://supabase.com/docs/guides/database/connecting-to-postgres), n.d.). Nếu buộc phải dùng (rất nhiều instance serverless), đặt `statement_cache_capacity(0)` (unnamed statements, xác nhận trên sqlx main/0.9; ⚠ có báo cáo lỗi với 0.8) và giữ `set_config(..., true)` trong transaction như hiện nay (đã an toàn với RLS). Với 1 spa thì **không cần**.
- PgBouncer riêng (Pro, chỉ transaction mode, IPv6 hoặc add-on IPv4 ~$4) không mang lại gì ở tải này.
- Kết nối direct (`db.<ref>.supabase.co`) chỉ có IPv6, bỏ được một hop proxy (~1–2 ms/câu). Đáng cân nhắc chỉ khi host API có IPv6 (Fly có, DO có).

### 4d. Engineering không đổi vendor (giải nguyên nhân 3 + 4, và bớt phần còn lại của 1)
| Việc | Gì | Hiệu quả dự kiến (từ VN, hiện tại) |
|---|---|---|
| **Bỏ waterfall hồ sơ khách** | `packages`, `summary`, `history`, `follow-ups` gọi song song từ `$customerId` trong URL, không chờ `/customers/{id}` | L4 → L3: **−1 lượt ≈ −0,3–0,45 s** |
| **Route loader + preload theo intent** | `defaultPreload: "intent"` + `loader: ensureQueryData(...)`: nạp chunk + query khi hover hoặc chạm nav, chạy song song với chunk | −1 lượt khi chuyển màn (≈ −0,3 s), gần như tức thì khi hover đủ lâu |
| **staleTime mặc định + focus** | `defaultOptions.queries.staleTime` ~30 s. Giữ refetch-on-focus nhưng chỉ khi stale (máy quầy + điện thoại KTV cùng sửa dữ liệu). staleTime 0 chỉ để lại cho dữ liệu cần tươi tuyệt đối. | Hết loạt 3–6 request mỗi lần quay lại tab |
| **Không await invalidate trước toast; optimistic update** cho thao tác đơn giản (chấm công, điểm ngày, follow-up, trạng thái lịch hẹn) | `onMutate` + rollback. Server vẫn quyết định; lỗi thì trả UI về như cũ. | Lưu lượt hiện toast sớm hơn **~0,3–0,45 s**. Ô chấm công phản hồi tức thì. |
| **`/quay?view=lich` không gọi `/visits`** khi không hiện | gate theo view | −1 request |
| **Gộp "ống nước" auth** | Một câu SQL lấy session + membership + permissions (hoặc dùng chung một `TenantTx` cho auth và nghiệp vụ) | 8–17 → ~4–11 câu/request: **−10–20 ms** bây giờ, quan trọng hơn nếu DB xa |
| **`Server-Timing`** (`db;dur=`, `app;dur=`, số câu SQL) | Đo được ngay trên DevTools của EM, không cần script | Quan sát, không tốn gì |
| **Mặc định `Cache-Control: private, no-store`** cho `/api` (layer `if_not_present`) | Sửa phát hiện bảo mật §3 | Bảo mật, không đổi tốc độ |
| **ETag + `private, no-cache`** cho catalog (`/services`, `/service-categories`, `/package-plans`, `/staff`) | `304` không body khi không đổi ([MDN caching](https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Caching), 2026-08; [RFC 9111](https://www.rfc-editor.org/rfc/rfc9111.html)). **Không bao giờ** `public`/`s-maxage` cho `/api` vì CDN sẽ chia sẻ dữ liệu giữa người dùng. | Ít: payload đã ≤ 2 KB br, vẫn tốn một lượt đi về. Ưu tiên thấp. |
| **"Day bundle" (BFF)** cho Quầy: `/me` + `/staff` + `/visits` trong một response | Giảm 3 → 1 request ở lần tải lạnh | Chỉ −1 lượt (hai request kia đang song song). Làm **sau** khi đổi vùng nếu vẫn cần. |

**Không đáng làm bây giờ:** thêm index (query 0,25–1,7 ms, bench 3 năm đạt ngân sách), tách bundle thêm (124,8/125 KB, hãy giữ ngân sách), HTTP/3 (Vercel chưa hỗ trợ theo [community](https://community.vercel.com/t/http-3-support/41935), ⚠ forum; h2 đã có), nén (đã có br), CDN cho asset (đã HIT ở `hkg1`/`sin1`, `immutable`).

**Giữ ấm (keep-warm ping):** cold start xuất hiện sau ~45 s nghỉ (§2e), nên phải ping dày, nhiều lần mỗi phút. Cron của Vercel Hobby chỉ chạy hằng ngày, còn GitHub schedule tối thiểu 5 phút và hay trễ. Đây là chắp vá. Phương án B/C giải gọn hơn.

## 5. Kế hoạch theo giai đoạn
**Giai đoạn 0: engineering tự làm, $0, ~2–3 ngày (không cần EM quyết)**
1. `Server-Timing` + đưa script đo vào `infra/perf/` (để đo lại trước/sau).
2. Mặc định `private, no-store` cho `/api` (secure).
3. Web: staleTime mặc định + focus khi stale; bỏ waterfall hồ sơ khách; `defaultPreload: "intent"` + loader `ensureQueryData` cho các màn lazy; không await invalidate trước toast; tắt `/visits` ở tab lịch; optimistic cho chấm công, điểm ngày, follow-up, trạng thái lịch hẹn.
4. API: gộp tra session + quyền thành một câu và dùng một transaction mỗi request.

**Trạng thái giai đoạn 0 (2026-11-07, PR `fix/api-cache-control-default` #80 + `perf/phase0-quick-wins`):**

| Việc | Trạng thái | Ghi chú / số đo |
|---|---|---|
| 1. `Server-Timing` + `infra/perf/` | ✅ | `db;dur;desc="n statements", total;dur` mọi response trừ `/auth/*` ([backend §5](backend.md)) |
| 2. `private, no-store` mặc định | ✅ #80 | [threat-model](../security/threat-model.md) TB1 |
| 3a. staleTime 30 s + focus khi stale | ✅ | gói của khách 5 s, lương 0; staff views vào `invalidateManagementViews` |
| 3b. Hồ sơ khách song song | ✅ | 5 request cùng lúc từ id trong URL (test `profilePrefetch.test.tsx`); L4 → L3 |
| 3c. `defaultPreload: "intent"` + loader | ✅ `/`, `/khach-hang`, `/khach-hang/$id`, `/bao-cao` | `/quay` cố ý không (LCP); `/nhan-vien` để sau (agent HR đang sửa) |
| 3d. Toast không chờ refetch; ghi thẳng câu trả lời | ✅ | ghi lượt, bán gói lẻ, hồ sơ, chấm công, điểm ngày, liên hệ, lịch hẹn: −1 lượt đi về mỗi lần lưu |
| 3e. Tab lịch không gọi `/visits` | ✅ | −1 request |
| 4. Gộp ống nước auth | ✅ | quyền 2 câu → 1; `BEGIN`+`set_config` 2 → 1 mỗi `TenantTx` (2 tx/request): **−3 câu SQL mỗi request đăng nhập** |

Bundle entry gzip -9: 124,76 → **124,99 KiB** (ngân sách 125; hết dư, PR sau phải bù). Chưa đo lại từ VN trên staging (cần deploy).

Mục tiêu đo lại (từ VN): số lượt nối tiếp mỗi màn ≤ 2, mỗi lần chuyển màn nhanh hơn ~0,3–0,5 s, không còn loạt request khi quay lại tab. Giai đoạn này **không** giải được phần ~200–400 ms mỗi request và cold start.

**Giai đoạn 1: đổi vùng, cần EM quyết (~1 ngày, cho staging)**
5. Tạo Supabase project `ap-southeast-1`, bootstrap + migrate + `seed-demo` (runbook [vercel-staging](../runbooks/vercel-staging.md)).
6. API chạy ở Singapore, **luôn chạy** (đề xuất B hoặc C, xem [ADR-0012](../decisions/ADR-0012-colocate-api-db-singapore.md)). Nếu EM muốn ít thay đổi nhất: A (Vercel `sin1`) là bước trung gian, nhưng vẫn còn cold start.
7. Đo lại bằng cùng script + Globalping. Mục tiêu: API p50 ≤ 120 ms, p95 ≤ 250 ms từ VN; lần bấm đầu sau 10 phút nghỉ ≤ 300 ms (với B/C).

**Giai đoạn 2: chỉ khi số đo sau giai đoạn 1 còn thiếu**
8. "Day bundle" cho Quầy, ETag catalog, kết nối direct IPv6 tới DB.

## 6. Cần EM quyết vs engineering tự làm
**Cần EM quyết (vendor / vùng / tiền):**
- **Q-P1 Vùng:** đồng ý dời staging sang Singapore (Supabase project mới + API Singapore)? Dữ liệu demo tạo lại được; mật khẩu và URL DB mới thì EM giữ như lần trước. Cũng là bước chuẩn bị cho production (ADR-0002/0005 đã chọn Singapore).
- **Q-P2 Nơi chạy API:** A (Vercel `sin1`, ~$0 thêm nhưng còn cold start 1,3–1,8 s, phải lên Pro $20 khi dùng thật), B (Fly.io `sin` luôn chạy, ~$5–9), hay C (VPS SGP1 $6–12, đúng ADR-0005, tự vận hành). **Khuyến nghị: B cho staging ngay** (ít vận hành, giữ một origin qua Vercel). Production chốt giữa B và C.
- **Q-P3 Vercel Pro ($20/tháng):** bắt buộc khi spa dùng thật nếu SPA hoặc API còn ở Vercel (Hobby cấm thương mại). Thay thế: Caddy trên VPS phục vụ luôn SPA (C) thì không cần Vercel.
- Supabase Pro ($25) đã có trong kế hoạch (credit, ADR-0002), không phát sinh mới.

**Engineering tự làm (không cần quyết):** toàn bộ giai đoạn 0. Riêng optimistic update chỉ áp dụng cho thao tác không đụng tiền; lưu lượt / bán gói vẫn chờ server.
