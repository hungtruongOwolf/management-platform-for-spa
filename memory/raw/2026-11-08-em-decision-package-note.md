# EM decision — 2026-11-08: a note per package sale (F4)

The lead explained that a cart holding only packages has no note field. A package sale has no note column, and a 2–3 package checkout is 2–3 separate sales. A desk note often holds health information, so it needs encryption and a read rule. It asked whether a note is needed when a customer only buys packages. Verbatim:

> thường khách chỉ mua 1 gói 1 lần thì ghi chú theo gói đi làm feature này

## Lead's reading
- Build it. Each package sold gets its own optional note, attached to that package sale. In a 2–3 package cart, each package line has its own note field, collapsed and optional, so no extra steps by default.
- The note is treated like a session note:
  - encrypted at rest;
  - readable only by holders of `customer.health_notes.read` (plus the sale/customer read the screen already needs);
  - never sent to technicians or former staff;
  - never logged.
- It shows where a package sale is read back: Quầy's "Bán gói không kèm lượt" block, the customer's history/timeline, the package detail, and Nhật ký ghi chú as a new kind ("Ghi chú gói"), with the same rule.
- A package sold inside a visit can also carry its own note, alongside the visit's session note.
