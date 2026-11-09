-- Sizes of schema app (db review): database, heap vs index, per table, the indexes 0033 drops.
SELECT pg_size_pretty(pg_database_size(current_database())) AS db,
       pg_size_pretty(sum(pg_relation_size(c.oid)) FILTER (WHERE c.relkind = 'r')) AS heap,
       pg_size_pretty(sum(pg_relation_size(c.oid)) FILTER (WHERE c.relkind = 'i')) AS indexes
FROM pg_class c WHERE c.relnamespace = 'app'::regnamespace;
SELECT c.relname, s.n_live_tup AS rows, pg_size_pretty(pg_relation_size(c.oid)) AS heap,
       pg_size_pretty(pg_indexes_size(c.oid)) AS indexes,
       (SELECT count(*) FROM pg_index i WHERE i.indrelid = c.oid) AS n_indexes
FROM pg_class c JOIN pg_stat_user_tables s ON s.relid = c.oid
WHERE s.schemaname = 'app' ORDER BY pg_total_relation_size(c.oid) DESC LIMIT 10;
SELECT c.relname AS index, pg_size_pretty(pg_relation_size(c.oid)) AS size
FROM pg_class c WHERE c.relnamespace = 'app'::regnamespace AND c.relname IN
  ('customer_search_trgm', 'customer_phone_trgm', 'visit_line_visit_idx', 'sale_line_sale_idx',
   'visit_day_idx', 'sale_day_idx', 'payment_day_idx', 'refund_day_idx')
ORDER BY pg_relation_size(c.oid) DESC;
SELECT pg_size_pretty(sum(pg_relation_size(c.oid))) AS dropped_total
FROM pg_class c WHERE c.relnamespace = 'app'::regnamespace AND c.relname IN
  ('customer_search_trgm', 'customer_phone_trgm', 'visit_line_visit_idx', 'sale_line_sale_idx',
   'visit_day_idx', 'sale_day_idx', 'payment_day_idx', 'refund_day_idx');
