# EM request — 2026-11-08: review the database design (tables, indexes, FKs, storage, integrity, latency)

The EM looked at the staging Supabase project (schema `app`, 43 tables) and asked. Verbatim:

> ok tôi có thấy thấy rồi nhưng có một chuyện nữa về cách cải thiện database design về tables index mọi thứ FK để tăng effiency và không cần lưu nhiều loại dẫn đến gây ảnh hưởng bộ nhớ và cũng như vẫn giữ được mấy cái logic handle được data coruption và conflict thì bạn check thử giúp tôi với xem xét kĩ lưỡng vì cái này vừa có thể giảm độ phức tạp vừa có thể tăng latency trong các câu query giúp response nhanh hơn

## Lead's reading
A thorough review of the Postgres schema, done before changing anything. It covers:
- **Tables and normalization:** redundant or duplicated data, columns stored that could be derived, overly wide rows, unused tables or columns, and JSON vs columns.
- **Indexes:**
  - missing indexes on hot queries and FK columns;
  - duplicate, redundant or unused indexes, which cost write time and storage;
  - partial and covering indexes;
  - the tenant-first ordering used with RLS.
- **Constraints:** FK coverage and ON DELETE behaviour, CHECK and UNIQUE/EXCLUDE constraints that protect against corruption and double-booking, NOT NULL, and enum/check vs lookup tables.
- **Concurrency and conflict handling:** row versions (If-Match), idempotency keys, advisory or row locks, unique constraints, and transaction isolation. Keep all of these, or make them stronger.
- **Storage:** table and index sizes, audit/idempotency retention, ciphertext sizes, and TOAST.
- **Latency:**
  - the slowest and most frequent queries, measured with EXPLAIN (ANALYZE, BUFFERS) on realistic data;
  - statements per request, N+1 patterns, and RLS overhead;
  - the connection pooler.
- **Output:** findings with evidence, ranked by impact and risk, plus a proposal (which migrations, in what order, expected gain). The EM decides before any schema change. The integrity and conflict logic must stay intact.
