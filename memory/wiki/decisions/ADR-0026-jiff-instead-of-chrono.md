---
title: ADR-0026 jiff thay chrono / chrono-tz cho ngày, giờ và múi giờ của API
status: accepted
date: 2026-11-28
deciders: EM ("Thay chrono ngay", 2026-11-27), lead (chọn thư viện), agent (hiện thực)
---

# ADR-0026: API dùng `jiff` 1.x thay `chrono` + `chrono-tz`; ranh giới DB qua `jiff-sqlx` + `sqlx.toml`, chữ trên dây giữ nguyên

## Context
- `cargo-deny` (bước CI "Rust lint") đỏ trên `main` và mọi PR Rust vì hai advisory **RUSTSEC-2026-0543** (`chrono`) và **RUSTSEC-2026-0555** (`chrono-tz`): loại *informational / unmaintained*, không có bản vá, khuyên chuyển sang `jiff` hoặc `time`.
- EM chọn "Thay chrono ngay" thay vì ignore trong `deny.toml` hay hạ mọi advisory unmaintained xuống warning ([nguồn](../../raw/2026-11-27-em-replace-chrono-now.md)). `deny.toml` không đổi.
- API cần: instant (`timestamptz`, 95 cột), ngày nghiệp vụ (`date`), giờ trên đồng hồ (`time`, giờ mở cửa, ca, chấm công), và **múi giờ IANA theo spa** (`app.tenant.timezone`, hôm nay `Asia/Ho_Chi_Minh`): ngày nghiệp vụ là ngày theo giờ địa phương của spa.
- Ràng buộc: hợp đồng API (`openapi.json`, `packages/api-client`) và chữ JSON (instant RFC 3339 UTC có `Z`, ngày `YYYY-MM-DD`) không đổi; JSON đã lưu (audit `before/after`, body idempotency) vẫn đọc được; build offline (`SQLX_OFFLINE=true`, `.sqlx/`) trong CI và Docker vẫn chạy.

## Options considered
| Option | Pros | Cons |
|---|---|---|
| A. `jiff` 1.x (BurntSushi) + `jiff-sqlx` 1.0 + `sqlx.toml` | API rõ (Timestamp / civil / SignedDuration / Span), có tzdb IANA đóng gói (`jiff-tzdb`), được bảo trì; `sqlx` 0.9 có `[macros.type-overrides]` | sqlx không có kiểu jiff gốc: phải dùng wrapper ở ranh giới DB; utoipa chưa có feature `jiff_1`; serde của jiff cắt số 0 cuối phần lẻ giây |
| B. `time` 0.3 + một crate tz | sqlx và utoipa hỗ trợ sẵn | `time` không có IANA tz: thêm một crate tz (bảo trì yếu hơn, tự đóng gói tzdb); đổi lại toàn bộ code y như A |
| C. Ignore hai advisory trong `deny.toml` | Không đổi code | EM từ chối; hai crate không còn ai bảo trì |
| D. Hạ mọi advisory unmaintained xuống warning | Không đổi code | EM từ chối; che cả advisory sau này |

## Decision
Chọn **A**.

**Kiểu trong domain** (code nghiệp vụ, DTO, row struct):
- instant: `jiff::Timestamp`; ngày: `jiff::civil::Date`; giờ trên đồng hồ: `jiff::civil::Time`;
- thời lượng tuyệt đối: `jiff::SignedDuration` (phiên, khóa đăng nhập, cửa sổ MFA / re-auth / step-up, độ dài lịch hẹn, "N × 24 giờ" trên instant);
- bước lịch trên **ngày** (`civil::Date`): `jiff::Span` (`1.day()`, `Span::new().months(n)`); tháng lương, hạn gói theo tháng, preset báo cáo; ngày 31 + 1 tháng rơi về ngày cuối tháng, như chrono;
- múi giờ: `jiff::tz::TimeZone`, đọc từ **tzdb đóng gói trong binary** (feature `tzdb-bundle-always`, không bật `tzdb-zoneinfo` / `tz-system`): ngày nghiệp vụ không phụ thuộc zoneinfo của image, đúng như `chrono-tz` trước đây. `TenantProfile` / `TenantFacts` giữ `TimeZone` (`Clone`, không còn `Copy`); `TenantProfile::zone_name()` cho SQL `AT TIME ZONE`.

**Ranh giới DB** (`jiff-sqlx` 1.0, feature `postgres`; `sqlx` bỏ feature `chrono`, thêm `sqlx-toml`):
- Mỗi crate có `query!` có một `sqlx.toml` giống hệt (`kernel/tests/sqlx_toml.rs` kiểm), key viết HOA đúng tên `PgTypeInfo`: `'TIMESTAMPTZ'`, `'DATE'`, `'TIME'` → `jiff_sqlx::Timestamp/Date/Time`, mảng `'…[]'` → `Vec<jiff_sqlx::…>`. Không có cột / tham số `interval` nào ra Rust nên không map (`jiff_sqlx::Span` chỉ decode).
- Tham số: `.to_sqlx()` (`jiff_sqlx::ToSqlx`), `Option` thì `.map(ToSqlx::to_sqlx)`; mảng `date[]` truyền `Vec<jiff_sqlx::Date>` (slice không được nhận).
- Cột: `query_as!` tự `.into()` nên cột NOT NULL vào thẳng field kiểu jiff; cột **nullable** phải là `Option<jiff_sqlx::…>` trong row (Rust không có `From` giữa hai `Option`), đổi bằng `.map(jiff_sqlx::Date::to_jiff)` nơi dùng. Bản ghi ẩn danh của `query!` luôn là wrapper. Query runtime: `.bind(jiff_sqlx::Date::from(d))`, `query_scalar::<_, jiff_sqlx::Timestamp>`.
- `.sqlx/` không đổi: override áp lúc mở macro từ tên kiểu đã lưu, offline build như cũ.

**Hợp đồng OpenAPI**: bật feature `jiff_0_2` của utoipa. utoipa-gen nhận diện kiểu theo **tên đoạn cuối** của path (`Timestamp` → `string, date-time`; `Date` → `string, date`), không kéo `jiff` 0.2 vào build, nên phủ cả jiff 1.x mà không cần `#[schema(value_type = …)]` trên từng field. (`civil::Time` không được nhận: field `Time` lộ ra schema phải ghi `value_type = String`; hiện không có.)

**Chữ trên dây**:
- Instant: `kernel::time::rfc3339` viết đúng chữ chrono từng viết (`SecondsFormat::AutoSi`, `Z`: không phần lẻ, hoặc 3 / 6 / 9 chữ số, ngắn nhất mà vẫn chính xác). Serializer mặc định của jiff cắt số 0 cuối (`.12Z` thay cho `.120Z`): vẫn RFC 3339 nhưng khác chữ, và web `byUseOrder` (`features/front-desk/api.ts`) so sánh `created_at` theo chuỗi. Vì vậy mọi field `Timestamp` được serialize mang `#[serde(with = "kernel::time::rfc3339")]` (field `Option`: `serialize_with = "kernel::time::rfc3339::serialize_option"`, parse vẫn của serde nên thiếu key vẫn là `None`); instant đưa thẳng vào `json!` (audit) dùng `rfc3339::format`. `kernel/tests/wire_instants.rs` đọc source và fail khi một field `Timestamp` của kiểu `Serialize` thiếu attribute.
- Ngày (`2026-11-05`) và giờ (`08:55:00`) của jiff ra cùng chữ với chrono (test trong `kernel::time`).
- Cursor giữ định dạng: instant 6 chữ số lẻ (`{t:.6}`), ngày `YYYY-MM-DD`.

**Quy tắc tính toán**: đọc đồng hồ địa phương một lần (`tz.to_datetime(ts)`) để biết ngày / giờ hiển thị; **không cộng trừ trên giờ địa phương**, mọi phép tính làm trên instant (`Timestamp ± SignedDuration`) hoặc trên `civil::Date` (± `Span` ngày / tháng). `Timestamp + Span` có đơn vị ngày trở lên **compile được nhưng panic lúc chạy**: không bao giờ dùng.

## Consequences
- Positive: `cargo tree -i chrono` / `-i chrono-tz` rỗng, `cargo-deny` xanh với `deny.toml` không đổi; `openapi.json` và `schema.ts` sinh lại không đổi byte; chữ JSON (DTO, audit, idempotency) không đổi, hàng cũ do chrono ghi vẫn parse (jiff đọc mọi offset và mọi số chữ số lẻ); tzdb vẫn nằm trong binary.
- Negative / accepted trade-offs:
  - Mỗi crate có `query!` mang một bản `sqlx.toml` (sqlx chỉ đọc file ở gốc crate); test giữ chúng giống nhau.
  - Ranh giới DB dài hơn chrono: `.to_sqlx()` ở tham số, `Option<jiff_sqlx::…>` trong row cho cột nullable.
  - Field `Timestamp` mới phải nhớ attribute serde; test source scan bắt chỗ quên.
  - `utoipa` feature tên `jiff_0_2` dùng cho jiff 1.x (khớp theo tên, ổn định nhưng dễ gây hiểu lầm khi đọc `Cargo.toml`).
  - Parser ngày của jiff rộng hơn chrono ở đầu vào lạ: nhận `2026-11-05T10:00+07:00` cho tham số ngày (lấy phần ngày), chrono từ chối. Đầu vào hợp lệ `YYYY-MM-DD` và mọi đầu ra không đổi.
  - `TenantProfile` / `TenantFacts` / demo `Clock` thành `Clone` thay vì `Copy` (clone `TimeZone` chỉ tăng một reference count).
- Follow-ups:
  - Crate `lucky_draw` (PR #205) đổi theo mục dưới sau khi PR này merge.
  - Nhân tiện: `identity` khai báo feature axum `matched-path` nó vẫn dùng (trước đây chỉ `app` bật, `cargo check -p identity` riêng lẻ không compile).

## Đổi một crate sang jiff (cho `lucky_draw` và crate mới)
1. `Cargo.toml`: bỏ `chrono` / `chrono-tz`; thêm `jiff = { version = "1.0.3", default-features = false, features = ["std", "serde"] }` và `jiff-sqlx = { version = "1.0.0", features = ["postgres"] }`; bỏ `"chrono"` khỏi feature của `sqlx`; `utoipa` dùng `features = ["uuid", "jiff_0_2"]`.
2. Chép `crates/kernel/sqlx.toml` vào gốc crate (không sửa; `kernel/tests/sqlx_toml.rs` so sánh).
3. Kiểu: `DateTime<Utc>` → `jiff::Timestamp`, `NaiveDate` → `jiff::civil::Date`, `NaiveTime` → `jiff::civil::Time`, `Utc::now()` → `Timestamp::now()`, `chrono_tz::Tz` → `&jiff::tz::TimeZone`, `BusinessDate::as_naive()` → `as_date()`, `of_instant(ts, &tz)`.
4. Thời lượng: `Duration::minutes/hours/seconds(n)` → `SignedDuration::from_mins/from_hours/from_secs(n)`; `ts - other` để so sánh → `ts.duration_since(other)`; `Duration::days(n)` trên instant → `SignedDuration::from_hours(24 * n)`, trên ngày → `Span::new().days(n)` (hoặc `n.days()` với `jiff::ToSpan`); `(a - b).num_days()` giữa hai ngày → `(a - b).get_days()` (`i32`).
5. Lịch: `with_day(1)` → `first_of_month()`, `pred_opt/succ_opt` → `yesterday()/tomorrow()` (`Result`), `checked_add_months(Months::new(m))` → `checked_add(Span::new().months(m))`, `weekday().num_days_from_monday()` → `weekday().to_monday_zero_offset()`, `year()` là `i16`, `month()/day()` là `i8`; `parse_from_str(s, f)` → `strptime(f, s)`, `format(f)` → `strftime(f)`; giờ địa phương: `tz.to_datetime(ts)`, ngược lại `tz.to_timestamp(date.at(h, m, 0, 0))`.
6. SQL: tham số `.to_sqlx()`; field nullable của row `query_as!` là `Option<jiff_sqlx::…>`; `query!` trả wrapper (`.to_jiff()`).
7. DTO: mỗi field `Timestamp` serialize thêm attribute `rfc3339` (mục Decision); instant trong `json!` dùng `kernel::time::rfc3339::format`.
8. `pnpm db:prepare` (`.sqlx`), `pnpm api:gen` rồi kiểm diff rỗng; `cargo test -p kernel --test sqlx_toml --test wire_instants`.

## Revisit when
utoipa có feature cho jiff 1.x (đổi `jiff_0_2`), sqlx hỗ trợ jiff gốc (bỏ wrapper và `sqlx.toml`), hoặc spa đầu tiên ở múi giờ có DST (rà lại mọi chỗ đọc đồng hồ địa phương: quy tắc "tính trên instant / civil date" vẫn đúng, nhưng giờ mở cửa và lịch hẹn quanh giờ chuyển cần test riêng).
