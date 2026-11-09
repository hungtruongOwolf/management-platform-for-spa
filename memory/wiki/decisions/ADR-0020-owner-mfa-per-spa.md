---
title: ADR-0020 Công tắc "bắt buộc xác thực 2 bước cho chủ spa" theo từng spa
status: accepted (EM chọn hướng 1, 2026-11-09; lead quyết chi tiết; đã làm ở migration 0050)
date: 2026-11-09
deciders: EM (what), lead + agent (how)
sources: [raw/2026-11-09-em-request-owner-mfa-toggle.md]
amends: ADR-0004
---

# ADR-0020: Công tắc MFA chủ spa theo từng spa (sửa ADR-0004)

## Context
- EM 2026-11-09 ([raw](../../raw/2026-11-09-em-request-owner-mfa-toggle.md)) hỏi console `/admin` có bật/tắt được bước xác thực thứ 2 của chủ spa không. Lead nêu rủi ro (chủ spa thấy ghi chú sức khỏe, lương, quyền hoàn tiền mà không bị hỏi lại mật khẩu, [ADR-0004 update 2026-11-05](ADR-0004-authentication.md)) và 3 hướng. **EM chọn hướng 1: công tắc theo từng spa**, mặc định BẬT, tắt cần step-up + lý do + nhật ký, bật lại dùng tiếp authenticator cũ.
- Trước đây MFA của chủ spa là cứng: role hệ thống `owner` có `requires_mfa` (CHECK `role_owner_requires_mfa`, 0006); `app.user_requires_mfa` chỉ nhìn role (0041); đăng nhập hỏi mã khi `account.mfa_enabled || user_requires_mfa` (ai đã enroll là luôn bị hỏi).
- Console đã có mẫu cho công tắc theo spa: cờ tính năng (`platform_set_feature`, 0039) và "Reset MFA" (`platform_account_action`, 0041). Mọi ghi của console đi qua hàm definer có `platform_gate(t, true)` ([architecture/platform-admin](../architecture/platform-admin.md)).

## Options considered
| Option | Pros | Cons |
|---|---|---|
| **1. Công tắc theo spa (`tenant.owner_mfa_required`)** | Một quyết định cho cả spa, dễ giải thích; operator thấy ngay spa nào tắt; giữ CHECK của role | Tắt là tắt cho mọi chủ spa của spa đó |
| 2. Công tắc theo tài khoản | Hẹp nhất | Thêm trạng thái trên `user_account` (bảng toàn cục) — tài khoản ở nhiều spa khó lý giải; EM không chọn |
| 3. Giữ bắt buộc, thêm "nhớ thiết bị 30 ngày" | Giữ an toàn | Cần cookie thiết bị + thu hồi; không phải điều EM hỏi |

## Decision
Chọn **1**. Chi tiết (lead quyết):

1. **Dữ liệu.** `app.tenant.owner_mfa_required boolean NOT NULL DEFAULT true` (0050): mọi spa đang có và spa mới đều BẬT. CHECK `role_owner_requires_mfa` giữ nguyên: role vẫn nói "role này cần MFA", spa nói "spa này có áp dụng không". `app_rw` chỉ có `SELECT` trên `tenant` nên không route spa nào ghi được cột này.
2. **Quy tắc yêu cầu MFA** (`app.user_requires_mfa`): một role `requires_mfa` chỉ tính ở spa có policy BẬT. **Nghiêm nhất thắng**: chủ spa ở spa A (BẬT) và spa B (TẮT) vẫn phải MFA. Giữ M3 của ADR-0004: policy của spa đang **tạm ngưng** vẫn tính. Thực tế chỉ role `owner` có `requires_mfa`, nên công tắc chỉ đổi chủ spa; nếu sau này role khác được đặt `requires_mfa` thì công tắc của spa cũng áp dụng cho nó.
3. **Đăng nhập hỏi MFA khi và chỉ khi `user_requires_mfa(user)`** (bỏ `account.mfa_enabled ||`). Hệ quả:
   - Policy TẮT: chủ spa đăng nhập chỉ bằng mật khẩu, không có session pending. Secret TOTP và mã khôi phục **giữ nguyên**; BẬT lại → lần đăng nhập sau hỏi mã của **chính authenticator cũ**, không enroll lại.
   - Chủ spa chưa từng enroll: khi policy BẬT, lần đăng nhập sau bị bắt enroll (như trước).
   - Quy tắc "ai đã tự bật MFA thì luôn phải nhập mã" (ADR-0004) **bị bỏ**. Thực tế chưa có đường nào để người không bắt buộc tự bật MFA (`/auth/mfa/*` chỉ nhận session pending, mà session pending chỉ sinh khi bắt buộc), nên chỉ ảnh hưởng người từng là chủ spa rồi đổi sang role khác: nay họ đăng nhập bằng mật khẩu, authenticator vẫn được giữ.
4. **Reauth (🔒)**: hỏi mã khi và chỉ khi MFA đang bắt buộc (cùng quy tắc với đăng nhập). Bắt buộc mà tài khoản chưa enroll (không đi tới được qua API, vì mọi đường làm yêu cầu MFA bật lên đều thu hồi session) → `401 auth.mfa_enrollment_required` thay vì chấp nhận chỉ mật khẩu.
5. **BẬT thu hồi session** (cùng ý với L4 của ADR-0004), trong chính hàm DB: mọi session (đầy đủ và pending) của thành viên spa có role `requires_mfa` mà yêu cầu MFA **chuyển từ không sang có**. Ai đã cần MFA nhờ spa khác giữ session (họ đã qua MFA). **TẮT không thu hồi gì.** Mỗi thành viên bị ảnh hưởng được khóa advisory (`app.user_mfa:<id>`, theo thứ tự id, cả hai chiều) trước khi đọc yêu cầu, nên hai công tắc của hai spa chung một chủ chạy song song vẫn quyết "chuyển sang có" trên dữ liệu đã commit.
6. **Ghi qua hàm definer** `app.platform_set_owner_mfa(token_hash, tenant, required) → (previous, revoked)`: `platform_gate(t, true)` (step-up kiểm trong DB), `SET search_path = app, pg_temp`, `REVOKE ALL FROM PUBLIC`, `GRANT EXECUTE TO app_rw`; khóa dòng `tenant` (`FOR UPDATE`). Extractor `Operator` kiểm step-up trước (`403 platform.step_up_required`). Hàm không nhận lý do: như mọi hàm ghi khác của console, hai dòng audit do crate `platform` ghi trong cùng transaction.
7. **API**: `PUT /api/v1/admin/tenants/{id}/owner-mfa {required, reason}` → `200 {tenant: TenantRow, revoked_sessions}`. Lý do 3–500 ký tự (như mọi lý do của console). Đặt lại giá trị đang có → **`409 tenant.owner_mfa_unchanged`** (cùng kiểu `tenant.status_unchanged`; không ghi audit, không thu hồi). Spa không có → `404`. `TenantRow` (danh sách + chi tiết, `app.platform_tenant_overview`) có thêm `owner_mfa_required`.
8. **Audit hai phía**: nền tảng `platform.tenant.owner_mfa` (`detail: {before, after, revoked_sessions}`, lý do); spa `tenant.owner_mfa_changed` (`actor_kind = 'platform_admin'`, before/after `{owner_mfa_required}`, lý do), như `account.mfa_reset_by_platform`.
9. **UI** `/admin/spa/$id`: panel "Bảo mật đăng nhập" với dòng "Bắt buộc xác thực 2 bước cho chủ spa", chip Đang bật / Đang tắt, nút Tắt / Bật → sheet lý do → step-up (như cờ tính năng). Khi TẮT: callout "Chủ spa đang đăng nhập chỉ bằng mật khẩu, không cần mã xác thực." Danh sách `/admin`: chip "Không MFA" cạnh trạng thái.

### Trường hợp biên đã quyết
- **Session pending lúc TẮT**: không thu hồi; người đó vẫn hoàn tất được bằng mã (hoặc enroll). Lần đăng nhập sau thì không bị hỏi.
- **Cấp role cần MFA khi policy TẮT** (`provisioning::add_user`, L4): không thu hồi session (người đó không nợ MFA). Khi BẬT, hàm của công tắc thu hồi.
- **Reset MFA (console / CLI `reset-mfa`) khi TẮT**: vẫn xóa secret + mã khôi phục và thu hồi session; lần đăng nhập sau chỉ cần mật khẩu, lần sau khi BẬT thì enroll lại. `409 account.mfa_not_enabled` vẫn dựa trên việc có secret (enroll), không dựa trên policy.
- **Tài khoản trong màn Tài khoản**: chip "Có xác thực hai bước" (`mfa_enabled`) nghĩa là đã enroll, kể cả khi spa đang TẮT.
- **Spa tạm ngưng**: vẫn bật/tắt được; policy của nó vẫn tính cho chủ spa ở spa khác (M3).
- **`/me`** không có trường MFA nào (không đổi). Ô mã ở hộp xác nhận lại của màn Nhân viên đổi nhãn thành "Mã xác thực (nếu đăng nhập có hỏi mã)".
- **Chủ spa xem nhật ký của spa**: chưa có màn nhật ký phía spa (permission `audit.read` có, chưa có endpoint); dòng `tenant.owner_mfa_changed` nằm trong `audit_log` của spa và hiện trong nhật ký spa của console (nhãn tiếng Việt).

## Consequences
- Positive: EM tắt được bước 2 cho một spa khó dùng authenticator mà không đụng spa khác; bật lại không phải enroll lại; mọi lần đổi có lý do + nhật ký hai phía; DB tự kiểm step-up.
- Negative / accepted trade-offs: spa TẮT thì ai có mật khẩu chủ spa là vào được sổ sách, lương, ghi chú sức khỏe và hoàn tiền (ADR-0004 update 2026-11-05 đã bỏ reauth cho thao tác nghiệp vụ). Còn lại: mật khẩu ≥ 10 ký tự + danh sách phổ biến, khóa tài khoản, cookie `__Host-` + SameSite + Origin, idle 12 giờ. Rủi ro còn mở: một lần đăng nhập đang chạy đúng lúc operator BẬT (giữa lúc đọc yêu cầu và lúc ghi session) có thể giữ một session không MFA tới khi hết hạn — cửa sổ vài ms, cần mật khẩu đúng; giống L4 hiện có.
- Follow-ups: nếu cần, nhãn "Không MFA" trên báo cáo bảo mật định kỳ; màn nhật ký phía spa (`audit.read`) để chủ spa tự thấy thao tác của nền tảng.

## Revisit when
Có sự cố dùng mật khẩu chủ spa bị lộ ở spa TẮT, có nhiều spa xin tắt (cân nhắc hướng 3 "nhớ thiết bị"), hoặc thêm role khác cần MFA.
