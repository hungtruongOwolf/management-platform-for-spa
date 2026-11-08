---
title: Threat model (R1)
updated: 2026-11-04
sources: [architecture/system.md, product/specs/front-desk-and-packages.md]
---

# Threat model R1 (STRIDE theo trust boundary)

**Tài sản cần bảo vệ:** tiền (thanh toán, công nợ, sổ quỹ) · số buổi liệu trình khách đã trả · dữ liệu khách (SĐT, ghi chú sức khỏe) · dữ liệu nhân sự và lương (R2) · tài khoản chủ spa và platform admin · tính cách ly giữa các spa.

**Tác nhân:**
- (A1) người ngoài trên internet;
- (A2) nhân viên có tài khoản nhưng vượt quyền (ví dụ KTV muốn xem lương người khác, quản lý muốn xóa khoản thu);
- (A3) người dùng của spa khác;
- (A4) kẻ chiếm được tài khoản (lộ mật khẩu, máy quầy không khóa);
- (A5) sự cố nội bộ: deploy lỗi, mất dữ liệu.

## TB1: Browser ↔ Caddy/API (internet)
| STRIDE | Mối đe dọa | Biện pháp | Test |
|---|---|---|---|
| S | Dò hoặc nhồi mật khẩu (A1) | Argon2id, rate limit IP + tài khoản, lỗi đồng nhất, MFA cho chủ/admin (ADR-0004) | Đăng nhập sai lần thứ N → bị chậm hoặc khóa tạm |
| S | Lộ mật khẩu chủ spa (A4): kẻ tấn công chỉ có mật khẩu | TOTP bắt buộc; session sau mật khẩu chỉ là **pending MFA** (10 phút, bị `CurrentSession`/`AuthUser` từ chối, không lộ spa/permission, không gọi được `logout-all`/reauth) | `tests/mfa.rs`: session pending gọi `/me`, `/services`, `/customers` → 401; mutation đã kiểm |
| S | Dò mã TOTP 6 số | Mã sai tính vào khóa tài khoản (tăng nguyên tử trong SQL); nhập lại mật khẩu không reset bộ đếm; rate limit IP `/auth/*` | 5 mã sai → khóa, mã đúng cũng bị từ chối; 3 sai + login lại + 2 sai → khóa |
| S | Phát lại mã TOTP đã thấy/bị phishing | Mỗi bước thời gian chỉ chấp nhận 1 lần (`mfa_last_step`, UPDATE nguyên tử); mã khôi phục dùng 1 lần | Cùng mã ở lần đăng nhập thứ hai → 401; mutation đã kiểm |
| S | Enroll trước chủ spa (TOFU) khi có mật khẩu mà owner chưa enroll | Enroll ngay lần đăng nhập đầu; đã enroll thì không enroll lại được (409), chỉ operator reset; xác nhận gắn đúng ciphertext đã kiểm mã | `enroll/start` sau khi đã bật → 409 |
| I | Lộ DB (dump/backup) làm lộ secret TOTP / mã khôi phục | Secret AES-256-GCM (`APP_DATA_KEY` ngoài DB), AAD gắn user id; mã khôi phục chỉ lưu SHA-256 (80-bit entropy) | Ciphertext không chứa secret; hash không chứa mã |
| S | Đánh cắp session qua XSS | Cookie `HttpOnly`; CSP nonce-based, không `unsafe-inline`; React escape; không `dangerouslySetInnerHTML` | CSP header có mặt; XSS payload trong tên khách hiển thị dạng text |
| S | CSRF | `SameSite=Strict` + kiểm tra `Origin` | Request ghi với `Origin` lạ → 403 |
| T | Sửa request trong DevTools: đổi `price`, `tenant_id`, `staff_id`, `sessions_remaining`, `is_paid` (A2) | DTO chỉ nhận trường được phép ghi; giá lấy từ bảng giá phía server; giảm giá cần permission + lý do; tenant/user từ session | Gửi `tenant_id`/`price` lạ → bị bỏ qua hoặc `422` |
| T | Gửi lại request (replay / bấm đôi) → ghi đôi tiền hoặc trừ đôi buổi | Idempotency-Key; khóa lạc quan (optimistic lock); `CHECK sessions_remaining >= 0` | Gửi lặp cùng key → 1 bản ghi; hai request song song trừ buổi cuối → 1 thành công |
| R | Quản lý chối đã hủy hoặc giảm giá | Audit log mọi thao tác tiền/buổi (ai, khi nào, trước/sau, lý do); payment append-only | Mỗi lệnh ghi đều có dòng audit tương ứng |
| I | IDOR: đổi ID trong URL để xem khách hoặc liệu trình của spa khác (A3) hay ghi chú sức khỏe khi không có quyền (A2) | Query luôn scope theo tenant + RLS; ghi chú sức khỏe bị loại khỏi response nếu thiếu permission; ID là UUIDv7 | Ma trận test: tenant B gọi ID của tenant A → 404; thiếu quyền → không có field `health_note` |
| I | CDN/proxy cache dữ liệu cá nhân (tên, SĐT, lương) vì response không đặt `Cache-Control` (Vercel tự gắn `public, max-age=0, must-revalidate`; phát hiện đo perf 2026-11-07) | Layer ngoài cùng `app::middleware::private_unless_set`: mọi response `/api` không tự đặt thì nhận `private, no-store` (kể cả 401/403/404/422/429); handler tự đặt (`no-store`) giữ nguyên. **Không bao giờ** `public`/`s-maxage` cho `/api` | `tests/cache_control.rs` (route 200/401/403/404/422 + giá trị của handler được giữ, một giá trị duy nhất), unit `middleware_rejections_are_private_no_store` (429, CSRF 403) |
| I | Lộ thông tin qua lỗi hoặc stack trace | Lỗi dạng problem+json chung chung; chi tiết chỉ nằm trong log | Gây lỗi 500 → response không có stack/SQL |
| D | Spam request làm chậm hệ thống | Rate limit theo IP (S1: 5/giây, burst 60) + theo route (S2+); body ≤ 64 KB; timeout 10 giây; phân trang có trần. **Ràng buộc:** IP lấy từ `X-Forwarded-For` (Caddy đặt), nên API **chỉ được nhận traffic từ Caddy**: không publish port API, chỉ lắng nghe trong network nội bộ của Docker | Vượt ngưỡng → 429 |
| E | Gọi endpoint quản trị bằng tài khoản thường (A2); route guard frontend bị bỏ qua | `can()` ở service layer cho mọi route; route không khai báo permission thì CI fail; endpoint platform admin tách riêng + MFA | Test mỗi route với role không đủ quyền → 403 |

## TB2: API ↔ Postgres (Supabase)
| STRIDE | Mối đe dọa | Biện pháp |
|---|---|---|
| S | Lộ chuỗi kết nối DB | Secret trong env của VPS (file quyền 600), không vào git; gitleaks trong CI; xoay mật khẩu khi nghi ngờ |
| T | SQL injection | Chỉ dùng `sqlx::query!` với tham số bind; sort/filter qua allowlist |
| T | Bug hoặc người sửa thẳng dữ liệu tiền | Role `app_rw` chỉ DML, không DDL; không có `UPDATE/DELETE` trên `payment`, `audit_log` (REVOKE); migration chạy bằng role riêng |
| I | Truy cập qua Supabase Data API / anon key | Data API tắt; bảng ở schema `app`; RLS bật trên mọi bảng (ADR-0002) |
| I | Bug quên lọc tenant | RLS theo `app.tenant_id` (ADR-0003) + newtype `TenantId` bắt buộc trong repo |
| D | Hết connection | Pool giới hạn; dùng Supavisor; timeout query |

## TB3: API ↔ R2 (backup)
| STRIDE | Mối đe dọa | Biện pháp |
|---|---|---|
| I | Lộ file backup | Bucket private; mã hóa file trước khi upload (age/gpg); key chỉ cho phép ghi vào bucket backup |
| T | Kẻ xâm nhập xóa backup | R2 bucket lock (`daily/` 35 ngày, `monthly/` 400 ngày) chặn xóa/ghi đè kể cả với token của VPS (R2 không có token chỉ-ghi); lifecycle và lock chỉ chỉnh từ dashboard của EM; VPS không bao giờ xóa backup đã hoàn tất (review L7) |
| D | Backup lỗi âm thầm | Alert nếu không có backup mới trong 26 giờ; diễn tập khôi phục hằng tháng |

## TB4: Máy chủ VPS
| Mối đe dọa | Biện pháp |
|---|---|
| Chiếm quyền SSH | Chỉ SSH key, tắt root login, fail2ban; firewall chỉ mở 80/443/SSH |
| Lỗ hổng OS/package | Cập nhật bảo mật tự động; image distroless, user non-root |
| Lỗ hổng dependency | `cargo audit`, `cargo deny`, `pnpm audit` trong CI; Renovate |

## Mối đe dọa nội bộ & vận hành
| Mối đe dọa | Biện pháp |
|---|---|
| Máy quầy bỏ quên, đang đăng nhập (A4) | Idle timeout 12 giờ; hạn tuyệt đối 30 ngày; nút khóa nhanh; MFA khi đăng nhập của chủ spa; audit mọi thao tác. **Rủi ro chấp nhận (EM 2026-11-05):** thao tác nghiệp vụ của chủ spa (hủy/sửa ngày đã chốt, hoàn tiền) không còn hỏi lại mật khẩu, nên máy chủ spa bỏ quên hoặc session chủ spa bị đánh cắp làm được các việc đó; xem ADR-0004 update 2026-11-05 |
| Dùng chung tài khoản → mất trách nhiệm | Mỗi người một tài khoản (vision rủi ro #5); audit theo user |
| Platform admin xem dữ liệu spa tùy ý | Mặc định không có quyền đọc; chủ spa cấp support grant 24 giờ (thu hồi được); mọi truy cập đều audit và chủ spa xem được (ADR-0003) |
| Mất dữ liệu (A5) | Backup Supabase + dump đêm ra R2 + diễn tập khôi phục |

## Security review r1 (2026-11-04): phát hiện → biện pháp → test
| ID | Phát hiện | Biện pháp | Test (mutation-check) |
|---|---|---|---|
| H1 | `switch-tenant` phát session mới với `recent_auth_at = now()` (DEFAULT của cột) và hạn tuyệt đối mới 30 ngày: session bị đánh cắp bỏ qua được 🔒 và sống mãi | `identity::service::Issue::{MfaPending, Authenticated, Rotated}` bắt mọi chỗ phát session nói rõ thời điểm xác thực; switch-tenant giữ `recent_auth_at` + `absolute_expires_at` cũ; migration 0011 bỏ DEFAULT của `recent_auth_at` | `auth.rs::a_rotated_session_needs_a_real_reauth`, `auth.rs::switch_tenant_keeps_absolute_expiry_and_reauth_time`, `auth.rs::login_marks_the_session_as_just_authenticated` |
| M2 | Nhập sổ cũ không giới hạn ngày; giá trong sổ do quản lý gõ thành trần hoàn tiền | **Đã thay (EM 2026-11-05, migration 0012):** bỏ cửa sổ nhập và `refund_cap`. Gói sổ cũ luôn thuộc ngày nhập (hôm nay), ngày cũ khóa tự động cho Manager; hoàn ≤ `sold_price`, chỉ Owner hoàn, có audit. **Rủi ro còn lại chấp nhận:** quản lý gõ giá sổ cao rồi chủ spa hoàn theo giá đó mà không đối chiếu; trần hoàn riêng cho gói sổ cũ để sau | `opening_balances.rs::anyone_who_may_import_imports_today_without_a_window_or_reauth`, `::an_opening_balance_refund_is_capped_by_its_book_price` |
| M3 | Spa bị tạm ngưng vẫn dùng được với session đang mở | `member_permissions` yêu cầu `tenant.status = 'active'` → `403 auth.not_a_member`; `user_requires_mfa` không còn bỏ qua spa bị ngưng | `auth.rs::suspending_a_spa_cuts_off_its_open_sessions` |
| L1 | Map bucket rate limit tăng vô hạn; IPv6 mỗi địa chỉ một bucket | Bucket IPv6 theo /64 (IPv4-mapped → IPv4); task dọn `retain_recent` mỗi 60 giây, tự dừng khi layer bị drop | `middleware::tests::*` (unit) |
| L2 | Biết SĐT chủ spa là khóa được tài khoản | Giữ khóa theo tài khoản (chống dò phân tán), thêm `app unlock --phone P`; lý do ở ADR-0004 (update 2026-11-04). Rủi ro DoS còn lại được chấp nhận | `auth.rs::operator_unlock_lifts_a_lockout` |
| L3 | TOFU khi enroll MFA lần đầu | Chỉ ghi nhận (ADR-0004); giảm thiểu bằng quy trình operator | — |
| L4 | Thăng lên role cần MFA không thu hồi session cũ | `add_user` thu hồi mọi session khi role được cấp có `requires_mfa` | `auth.rs::promotion_to_an_mfa_role_ends_existing_sessions` |
| L5 | Từ khóa tìm khách (SĐT/tên) nằm trong access log | Caddy `format filter`: `request>uri regexp "\?.*$" "?[redacted]"`, xóa `Referer`; span tracing của API chỉ ghi `path` | `infra/prodtest/run.sh` ("access log drops query strings"); kiểm thủ công bằng `caddy:2.10-alpine` |
| L6 | Không kiểm TLS DB / quyền role khi khởi động | Ngoài development, `APP_DATABASE_URL` phải `sslmode=verify-full` hoặc `verify-ca` (miễn trừ duy nhất: `APP_DB_INSECURE_TLS_FOR_LOCAL_TEST=true`, chỉ staging, chỉ prodtest); `db_guard::check_runtime_role` từ chối superuser, BYPASSRLS, role sở hữu bảng trong `app` | `config::tests::*`, `tests/runtime_role.rs` |
| L7 | Backup xóa được từ VPS | R2 không có token chỉ-ghi → bucket lock `daily/` 35 ngày, `monthly/` 400 ngày; lifecycle do R2 chạy; script không còn bước xóa backup cũ; upload vào `incoming/` rồi mới chép sang `daily/` | Runbook §5, §7 |
| L8 | Action CI không pin SHA; deploy job có `packages: write` và chuyển token đó lên VPS | Pin mọi action theo commit SHA (`# vX`); `packages: write` chỉ ở job build, job deploy `packages: read` | `actionlint` sạch |
| INFO | `safeNextPath` nhận `/\t/evil.com`; hash idempotency không gắn route; `POST /sales` thu tiền không cần `payment.collect`; phân bổ giảm giá có thể vượt giá dòng | Chặn `[\x00-\x20\\]`; `kernel::idempotency::hash_route` (method + route + record + body) cho mọi endpoint idempotent (sales, packages, operations); `payment.collect` khi có payments; phần dư phân bổ không vượt giá dòng | `api.test.ts`, `idempotency::tests::route_hash_binds_method_route_and_record`, `sales.rs::taking_money_needs_payment_collect`, `domain::tests::shares_always_sum_to_the_discount_and_fit_their_lines` |

## Quản lý nhân viên (0016, secure review 2026-11-05)
| Lạm dụng | Kiểm soát (server) | Test |
|---|---|---|
| Quản lý/role tùy biến đọc lương qua hồ sơ | `pay` chỉ khi `payroll.read`; ghi lương cần cả hai quyền | `staff.rs::salary_is_only_sent_to_payroll_readers` (mutation-check) |
| KTV đổi staff id để xem lương người khác | `/me/staff` không nhận id; staff từ `staff.user_id` của session | `a_technician_sees_only_their_own_month` |
| Gửi `staff_id` của spa khác trong body | kiểm danh sách hồ sơ trong `TenantTx` → 422; FK `(tenant_id, staff_id)`; RLS | `another_spa_cannot_reach_our_staff` |
| Sửa công/thưởng của tháng đã trả lương để đổi lịch sử | khóa `payroll_period` + trigger; bản chụp chỉ thêm; không mở lại | `a_closed_month_is_locked_and_corrected_by_adjustments` |
| Ghi đúng lúc đang chốt (race) | ghi: FOR SHARE; chốt: FOR UPDATE trên cùng dòng | thiết kế `hr::period` (trigger là lớp chặn sau) |
| Giả người ghi | `recorded_by/created_by/closed_by` lấy từ session; body có field lạ → 422 | matrix + DTO `deny_unknown_fields` |
| Gửi lại lệnh chốt / thưởng | `Idempotency-Key` (replay giống hệt, khác body → 422) | `a_closed_month…` (replay) |
| Quy tắc lương độc hại (số âm, key lạ) | `PayrollRules::validate`, `deny_unknown_fields` | `rules_and_attendance_drive_deductions` |
| Lộ lương qua cache trình duyệt/proxy | `Cache-Control: no-store` mọi response hr/org-profile | `salary_is_only…` (header) |
| Người đã nghỉ (vẫn đăng nhập, ADR-0014) dùng quyền role cũ: xem khách, Quầy, lịch | quyền suy ra mỗi request từ `staff.status = 'left'` → chỉ `staff.read_own_past`; deny by default mọi route khác; session cũ đổi quyền ngay | `authz_matrix.rs::a_former_staff_member_reaches_only_their_past_pay`, `staff.rs::a_leaver_keeps_only_their_own_past_pay` |
| Người đã nghỉ xem tháng sau khi nghỉ (dữ liệu sau ngày nghỉ) | `GET /me/staff`/`day` kiểm tháng có trong đợt làm việc → `403 staff.not_employed_in_month` | `a_leaver_keeps_only_their_own_past_pay` |
| Gắn tài khoản chủ spa vào hồ sơ đã nghỉ để khóa chủ spa | `effective_permissions` không bao giờ hạ quyền người có `staff.manage` | `the_owners_login_is_never_downgraded` |
| KTV đọc ghi chú buổi (có thể có thông tin sức khỏe) của lượt người khác làm | `note` chỉ khi `visit_line.staff_id = own_staff_id` (SQL), allowlist key | `a_technician_sees_only_their_own_calendar`, `…_own_day_in_detail` (test âm) |
| Quản lý chưa gắn hồ sơ tự chấm công / chấm điểm cho mình (F19, 2026-11-08) | vai trò lấy từ tài khoản, không từ chức danh (EM 2026-11-08): tài khoản quản lý chưa gắn hồ sơ không ghi gì (`403 staff.link_required`, chủ spa miễn); chủ spa thấy cảnh báo tài khoản quản lý chưa gắn | `staff.rs::a_manager_without_a_linked_profile_records_nothing` (mutation-check: bỏ `link_required` → đỏ) |
| Chức danh tự do mâu thuẫn với quyền (ghi "Quản lý" cho tài khoản KTV hoặc ngược lại) | hồ sơ đã gắn hiện tên role của tài khoản, suy ra lúc đọc ở `org::repo`; chức danh không quyết định quyền nào | `staff.rs::a_linked_profile_shows_its_account_role` |
| Quản lý đọc sổ của chính mình qua sổ quản lý thay vì `/me` (EM 2026-11-08: không hiện chính mình) | server bỏ hồ sơ gắn với session khỏi mọi sổ đọc + `404` cho `/staff/{id}`, `/evaluations/history` của mình; lương của mình chỉ ở `/me/staff` | `staff.rs::a_manager_reads_their_own_month_only_through_me` |
| Mật khẩu tạm bị lộ / dùng lâu dài (ADR-0016) | CSPRNG 60 bit, chỉ trong response tạo / đặt lại (`no-store`), chỉ lưu hash, không audit / log; tài khoản `must_change_password` chỉ đổi được mật khẩu (cổng ở `resolve`) | `staff_logins.rs::the_owner_onboards_a_manager_who_must_set_their_own_password`, `::passwords_never_reach_the_logs` |
| Chủ spa A chiếm tài khoản của người ở spa B (gắn SĐT có sẵn rồi đặt lại mật khẩu) | tạo tài khoản chỉ với SĐT chưa có tài khoản (`409 account.phone_taken`); đặt lại từ chối tài khoản còn thuộc spa khác (`422 account.other_spa`) | `::a_login_is_created_only_for_a_new_phone_and_a_working_profile`, `::an_owner_reset_ends_every_session_and_forces_a_change` |
| Phiên bị đánh cắp dùng để tạo / đặt lại tài khoản | re-auth 5 phút (`require_recent_auth`) + MFA của chủ spa | `::owner_login_actions_need_a_recent_reauth` |
| Đặt lại mật khẩu mở lại người đã nghỉ bị khóa | `409 account.locked`; chỉ "Nhận lại làm" mở khóa (ADR-0014) | `::an_owner_reset_ends_every_session_and_forces_a_change` |
| Dò SĐT có tài khoản qua "Quên mật khẩu?" / làm phiền chủ spa | cùng `202` + cùng lượng hashing mọi nhánh; giới hạn `/auth/*` theo IP; một yêu cầu mở + một mỗi giờ mỗi tài khoản mỗi spa; chủ spa không có yêu cầu | `::forgotten_password_requests_reach_only_the_owner`, `::forgotten_password_requests_are_rate_limited` |
| Đổi mật khẩu bằng phiên bị đánh cắp | cần mật khẩu hiện tại (sai đếm vào khóa); thu hồi mọi phiên khác | `::the_owner_onboards_…`, `::an_owner_reset_…` |
| Từ chối nghỉ việc lộ dữ liệu khách (F24 liệt kê lịch hẹn) | chỉ sau `staff.manage`; chỉ ngày, giờ, tên khách như lịch; không SĐT / ghi chú | `the_roster_hires_ends_and_rehires_without_losing_history` (body `details` đúng tập key) |
Không có Critical/High. Còn mở: quản lý thấy số tiền thưởng/phạt mình ghi (Q-S3); chủ spa gắn nhầm tài khoản → người đó thấy phiếu lương hồ sơ đó (có audit).

## Nhật ký ghi chú (`GET /notes`, secure review 2026-11-07, [spec §10](../product/specs/management-views.md))
Một màn đọc mọi ghi chú gom nhiều nguồn, trong đó có dữ liệu sức khỏe (mã hóa) và lý do nghỉ việc: bề mặt "gom dữ liệu" mới, nên mỗi nguồn giữ quyền riêng.
| Lạm dụng | Kiểm soát (server) | Test |
|---|---|---|
| Quản lý không có `customer.health_notes.read` đọc ghi chú buổi / sức khỏe qua màn gom | cờ quyền per-loại tắt nhánh SQL; không giải mã; `kind=health_note` hay `q=` cũng trống | `notes.rs::a_manager_without_the_health_permission_sees_no_health_notes` |
| Một quyền "nhẹ" mở loại cần nhiều quyền | mỗi loại = AND đúng các quyền của màn gốc; bỏ một quyền → mất loại | `notes.rs::every_kind_keeps_its_own_read_rule` (9 loại × từng quyền) |
| Quản lý đọc lý do nghỉ việc | loại `employment_end` cần `staff.manage` | `each_role_reads_exactly_the_notes_it_may` |
| Quản lý đọc ghi chú nhân sự (`staff.note`) qua nhật ký, vượt allowlist 14 key của Nhân sự (Q-N1) | loại `staff_note` cần `staff.manage` | `each_role_reads_exactly_the_notes_it_may` (không có chữ ghi chú nhân sự trong body quản lý) |
| Role chỉ có `sale.read` đọc lý do giảm giá kèm tên khách (Q-N1) | hóa đơn ngoài lượt cần thêm `customer.read`; hóa đơn của lượt theo `visit.read` | `every_kind_keeps_its_own_read_rule` (nhánh giảm giá) |
| KTV mở màn | `report.read` | matrix + `each_role…` (403) |
| Spa khác dò id khách / nhân viên | `tenant_id = $1` ở mọi nhánh + RLS; id lạ → danh sách trống | `another_spa_sees_none_of_our_notes` |
| Giải mã hàng loạt (DoS) qua tìm chữ | khoảng ngày ≤ 366, quét ≤ 500 ghi chú / request, cursor đi tiếp; rate limit chung | `notes.rs::filters_search_and_pages_with_a_cursor` |
| Bản rõ lọt ra log / audit / cache | đọc không ghi audit; lỗi giải mã chỉ ghi id; `private, no-store`; `q` ở query string (log truy cập không ghi query) | `each_role…` (audit trống, header) |
Không có Critical/High. Còn mở: `q` có thể chứa chữ của ghi chú sức khỏe trong URL lịch sử trình duyệt của chính người dùng (như mọi ô tìm trên URL; chấp nhận).

## Còn mở
- Danh sách thao tác yêu cầu nhập lại mật khẩu: chỉ thao tác bảo mật tài khoản (đổi mật khẩu, cài đặt MFA), EM 2026-11-05. Khi thêm route đó: dùng `AuthUser::require_recent_auth` + test `401`.
