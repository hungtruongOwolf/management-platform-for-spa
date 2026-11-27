---
title: "Thiết kế: Vòng quay may mắn (API, DB, web)"
status: approved (lead 2026-11-27); agent build cập nhật mục "Khi build" nếu lệch
updated: 2026-11-27
sources: [../product/specs/lucky-draw.md, raw/2026-11-27-em-request-lucky-draw.md]
related: [backend.md, data.md, frontend.md, ../security/authz-matrix.md, ../design/design-system.md]
---

# Vòng quay may mắn: thiết kế

Spec: [lucky-draw](../product/specs/lucky-draw.md). Trang này là **hợp đồng** giữa backend và web; đổi shape thì sửa ở đây trước.

## 1. Module
- Crate mới **`lucky_draw`** (ADR-0006: mỗi bounded context một crate): `domain` (hàm thuần: đủ điều kiện, còn lại, xong chưa, chọn ngẫu nhiên đều từ một nguồn ngẫu nhiên tiêm vào), `service` (use case + SQL), `http` (route + OpenAPI). Đăng ký route ở `app`. RNG: `rand::rngs::OsRng` (hoặc `getrandom`) ở tầng service, test tiêm RNG cố định.
- Permission mới `lucky_draw.manage` (`kernel::permission::Permission::LuckyDrawManage`): role owner của mọi spa + mẫu vai trò `owner` (migration, như `record.history.read` ở 0052), danh mục permission của console `/admin` biết tên này (không `422 unknown_permission`).
- Audit (typed `kernel::audit::Action`, mỗi action có nhóm + câu ở web): `lucky_draw.create`, `lucky_draw.rename`, `lucky_draw.delete`, `lucky_draw.participants_add`, `lucky_draw.participant_update`, `lucky_draw.participant_delete`, `lucky_draw.prize_create`, `lucky_draw.prize_update`, `lucky_draw.prize_delete`, `lucky_draw.spin`, `lucky_draw.forfeit`. Lịch sử trên từng bản ghi (ADR-0022): cả 11 action **không** có mục lịch sử tại chỗ (`RecordTrace` → `null`); Nhật ký thao tác (owner, sau cờ) có câu tiếng Việt.

## 2. DB (migration 0055)
```
lucky_draw              (tenant_id, id uuid pk, name text 1..80, created_by → user_account, created_at, updated_at, version bigint)
lucky_draw_participant  (tenant_id, id, draw_id → lucky_draw ON DELETE CASCADE, name text 1..120,
                         gender text CHECK in ('male','female'), position int, created_by, created_at, updated_at, version)
lucky_draw_prize        (tenant_id, id, draw_id → … CASCADE, name 1..140, description ≤ 250 NULL, eligible CHECK in ('all','male','female'),
                         quantity int CHECK 1..1000, position int, created_by, created_at, updated_at, version)
lucky_draw_result       (tenant_id, id, draw_id → … CASCADE, prize_id → lucky_draw_prize, participant_id → lucky_draw_participant,
                         seq int > 0, drawn_at, drawn_by, forfeited_at NULL, forfeited_by NULL)
```
- FK luôn kèm `tenant_id` (`(tenant_id, draw_id) → lucky_draw (tenant_id, id)`), RLS `tenant_isolation` trên cả 4 bảng, `app_rw` chỉ quyền cần dùng.
- Kết quả: `UNIQUE (tenant_id, draw_id, participant_id)`, `UNIQUE (tenant_id, draw_id, seq)`, CHECK `(forfeited_at IS NULL) = (forfeited_by IS NULL)`; trigger: chỉ cho `UPDATE` đặt `forfeited_*` từ NULL một lần, mọi cột khác bất biến; prize / participant của kết quả phải cùng `draw_id` (FK ba cột `(tenant_id, draw_id, prize_id)` hoặc trigger). Prize / participant có kết quả không xóa được (FK `ON DELETE RESTRICT` từ result; xóa chương trình vẫn CASCADE vì cả cây đi cùng).
- Index: `lucky_draw (tenant_id, created_at DESC)`, con theo `(tenant_id, draw_id)`.
- Thêm vào `app.invariant_violations()` nếu rẻ: số kết quả chưa trả của một giải ≤ `quantity`; người trúng khớp giới của giải.

## 3. API (`/api/v1`, mọi route `lucky_draw.manage`, id lạ / spa khác `404`)
| Route | Thân | Trả |
|---|---|---|
| `GET /lucky-draws` | — | `200 {items: LuckyDrawSummary[]}` |
| `POST /lucky-draws` | `{name}` | `201 LuckyDrawDetail` |
| `GET /lucky-draws/{id}` | — | `200 LuckyDrawDetail` + `ETag` |
| `PUT /lucky-draws/{id}` (If-Match) | `{name}` | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}` | — | `204` |
| `POST /lucky-draws/{id}/participants` | `{participants: [{name, gender}]}` 1..500 | `201 LuckyDrawDetail` |
| `PUT /lucky-draws/{id}/participants/{pid}` (If-Match = version người đó) | `{name, gender}` | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}/participants/{pid}` | — | `200 LuckyDrawDetail` |
| `POST /lucky-draws/{id}/prizes` | `{name, description?, eligible, quantity}` | `201 LuckyDrawDetail` |
| `PUT /lucky-draws/{id}/prizes/{qid}` (If-Match) | như trên | `200 LuckyDrawDetail` |
| `DELETE /lucky-draws/{id}/prizes/{qid}` | — | `200 LuckyDrawDetail` |
| `POST /lucky-draws/{id}/spins` (**Idempotency-Key** bắt buộc) | `{prize_id}` | `201 SpinOutcome` |
| `POST /lucky-draws/{id}/results/{rid}/forfeit` | — | `200 LuckyDrawDetail` |

Mọi lệnh ghi trả **toàn bộ** `LuckyDrawDetail` (web thay cache một lần, không phải gọi lại `GET`). Route `POST participants` có giới hạn thân riêng 256 KB (mặc định 64 KB không đủ 500 tên dài); web chia danh sách dán thành lô ≤ 500, gửi tuần tự.

```ts
LuckyDrawSummary = { id, name, created_at, participant_count, prize_slots /* tổng quantity */, awarded /* kết quả chưa trả */, status: "not_started" | "in_progress" | "finished" }
LuckyDrawDetail  = { id, name, created_at, version, status, finished: boolean,
  participants: { id, name, gender: "male"|"female", position, version, result_id: string|null }[],   // thứ tự position
  prizes:       { id, name, description: string|null, eligible: "all"|"male"|"female", quantity, awarded, remaining,
                  eligible_count /* người còn đủ điều kiện */, position, version }[],
  results:      { id, seq, prize_id, participant_id, drawn_at, forfeited_at: string|null }[] }           // thứ tự seq
SpinOutcome = { result: { id, seq, prize_id, participant_id, drawn_at }, draw: LuckyDrawDetail }
```
- `status` / `finished` tính ở server bằng đúng một hàm (`domain`): `finished` = có ≥ 1 kết quả và không giải nào có `remaining > 0 && eligible_count > 0`.
- Lỗi (`problem+json` `code`): `validation_failed` (+ `errors[].field`: `name`, `participants[i].name`, `participants[i].gender`, `participants` (quá số lượng), `description`, `eligible`, `quantity`, `prizes` (> 50)), `lucky_draw.participant_drawn` 409, `lucky_draw.prize_drawn` 409, `lucky_draw.quantity_below_awarded` 422, `lucky_draw.prize_exhausted` 409, `lucky_draw.no_eligible` 409, `lucky_draw.result_forfeited` 409, cùng bộ `precondition.if_match_required` 400 / `conflict.stale_version` 412 / `idempotency.*` như các route khác.
- **Quay**: một transaction: `SELECT … FROM lucky_draw … FOR UPDATE` (mọi lệnh ghi của chương trình đều khóa dòng này trước) → đọc giải, `remaining`, nhóm đủ điều kiện → chọn `idx = uniform(0, n)` bằng OS RNG (không modulo bias) → `INSERT result (seq = max + 1)` → audit `lucky_draw.spin` (`after`: seq, tên giải, tên người trúng) → commit. Phản hồi idempotent lưu `{result_id}` rồi đọc lại khi replay (không lưu tên trong bản ghi idempotency).
- `version` của chương trình tăng ở **mọi** lệnh ghi trong chương trình (đổi tên, người, giải, quay, trả giải) để `ETag` của `GET` đổi theo; `PUT /lucky-draws/{id}` dùng nó cho `If-Match`.

## 4. Web
- Thư mục `apps/web/src/features/lucky-draw/`; màn lazy `LuckyDrawListPage` (`/vong-quay`), `LuckyDrawPage` (`/vong-quay/$drawId`); đăng ký ở `app/screens.ts` / router / `mayOpen` / `navFor` (nhóm Khách hàng, sau "Khách lâu chưa quay lại") / command palette. Không import gì từ màn này vào startup.
- Query key `["lucky-draws"]`, `["lucky-draws", id]`; mọi mutation `setQueryData` bằng `LuckyDrawDetail` trả về + invalidate danh sách.
- `Wheel` component: hai ảnh `wheel-disk.webp` (xoay, `border-radius: 50%`) + `wheel-frame.webp` (đứng yên) theo tỉ lệ vị trí của bản gốc (`left 8.798%`, `top 13.929%`, `width 80.687%`, `height 78.170%`, khung 466 × 481). Xoay: nhấn Quay → bắt đầu xoay ngay (CSS animation vô hạn), khi có `SpinOutcome` → giảm tốc tới sector ngẫu nhiên (8 sector, `crypto.getRandomValues`) trong ~4 s `cubic-bezier(.14,.72,.12,1)`; giữ kết quả tới khi `transitionend` mới đưa vào UI. Reduced motion: không xoay. Ảnh copy từ bản đã nén (lead để ở scratchpad), import qua Vite.
- Hàm thuần có Vitest: parse danh sách dán (`parseRoster`), chọn giải mặc định (`defaultPrize`), gom tổng kết theo giải (`summaryByPrize`), CSV (`winnersCsv` có chống injection).
- e2e `lucky-draw.spec.ts`: chủ spa tạo chương trình → dán 6 người (3 Nữ, 3 Nam) → 2 giải (Chỉ nữ × 2, Cả hai × 2) → quay tới hết → tổng kết đúng giới → trả giải một người → quay lại → xóa chương trình (e2e không để lại dữ liệu); quản lý không thấy nav, mở URL → màn 403 của app; axe sạch.

## 5. Khi build
*(agent ghi lệch thiết kế, số đo, quyết định K-nhỏ ở đây)*
