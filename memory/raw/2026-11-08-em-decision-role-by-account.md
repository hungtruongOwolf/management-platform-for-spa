# EM decisions — 2026-11-08: who is a manager comes from the account role; managers keep "Khách của tôi"

After PR #95 (manager self-view, position-text guard for F19), the lead asked two questions. Verbatim:

> - Nhận diện quản lý: giữ cách dựa vào chữ "Quản lý" trong chức danh, hay thêm ô chọn vai trò cố định (KTV / Quản lý) trong hồ sơ? chúng ta chia theo role mà nên chắc chắn là sẽ có các role as account thì theo cái nào thì làm cái đó
> - Quản lý có giữ mục "Khách của tôi" không? quản lý cũng phải làm cho khách như KTV mà nên chắc chắn là có rồi

## Lead's reading
1. **Role comes from the account, not from free text.** Every staff member will have an account, and that account's role (owner / manager / technician) decides what they may do and how they are treated, including decision C.
   - Drop the position-text heuristic from #95 ("Quản lý" in the title).
   - A profile's role is the role of the login linked to it. The position shown should follow that role, so it can't contradict it.
   - Safety for F19: a login whose role supervises staff (manager) but is not linked to a profile can't record attendance, praise/complaints, bonuses or evaluations. The app can't tell which profile is theirs, so it can't stop them recording for themselves. The owner gets a clear prompt to link.
   - Unlinked profiles (no account yet) count as technicians, and the owner is nudged to give everyone an account.
2. **Managers keep "Khách của tôi".** Managers also serve customers, like technicians. This is already shipped in #95; it is now confirmed.
