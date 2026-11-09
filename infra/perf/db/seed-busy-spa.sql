-- Seeds ONE throwaway database with a busy small spa: ~3 years, 30–80 visits a day, package sales,
-- appointments, attendance and evaluations for 10 staff, audit, sessions, idempotency records.
-- For EXPLAIN (ANALYZE, BUFFERS) and size measurements only (db review 2026-11).
--
-- NEVER run this against a shared, staging or production database. Use a fresh local database
-- migrated to the latest schema, as the schema owner:
--   createdb vpyd_dbreview; psql -1 -f each migration in order; psql -f seed-busy-spa.sql
-- Variables: -v days=1095 -v customers=8000 (defaults). ~3 years ≈ 400 MB with indexes, a few minutes.
--
-- Rows go straight into the tables, shaped like the use cases write them. Every constraint and
-- trigger of the schema still applies (FKs, CHECKs, EXCLUDE, append-only, closed periods).
-- ANALYZE runs after each bulk step: without it the FK checks keep plans made for empty tables
-- (sequential scans per inserted row) and the seed takes hours instead of minutes.

\set ON_ERROR_STOP 1
\if :{?days}
\else
\set days 1095
\endif
\if :{?customers}
\else
\set customers 8000
\endif

SELECT setseed(0.42);
SET client_min_messages = warning;
BEGIN;

-- ------------------------------------------------------------------ org, identity, staff
CREATE TEMP TABLE ctx AS
SELECT gen_random_uuid() AS tenant, gen_random_uuid() AS branch,
       gen_random_uuid() AS owner_user, gen_random_uuid() AS manager_user, gen_random_uuid() AS tech_user,
       gen_random_uuid() AS owner_role, gen_random_uuid() AS manager_role, gen_random_uuid() AS tech_role,
       (current_date - :days)::date AS first_day;

INSERT INTO app.tenant (id, name, slug) SELECT tenant, 'Spa Bench', 'spa-bench' FROM ctx;
INSERT INTO app.branch (id, tenant_id, name, created_at) SELECT branch, tenant, 'Chi nhánh chính', first_day FROM ctx;
INSERT INTO app.role (id, tenant_id, key, name, is_system, requires_mfa)
SELECT owner_role, tenant, 'owner', 'Chủ spa', true, true FROM ctx
UNION ALL SELECT manager_role, tenant, 'manager', 'Quản lý', true, false FROM ctx
UNION ALL SELECT tech_role, tenant, 'technician', 'Kỹ thuật viên', true, false FROM ctx;

CREATE TEMP TABLE perms (p text);
INSERT INTO perms VALUES
 ('customer.read'), ('customer.write'), ('customer.health_notes.read'), ('customer.health_notes.write'),
 ('customer.follow_up'), ('visit.read'), ('visit.record'), ('visit.backdate'), ('sale.read'),
 ('package.sell'), ('package.extend_expiry'), ('package.cancel'), ('payment.collect'),
 ('record.void_same_day'), ('record.void_closed_day'), ('catalog.manage'), ('report.read'),
 ('report.revenue.read'), ('report.long_range'), ('appointment.manage'), ('appointment.settings'),
 ('staff.manage'), ('staff.read'), ('staff.read_own'), ('attendance.record'), ('evaluation.record'),
 ('payroll.read'), ('payroll.close'), ('opening_balance.import'), ('sale.discount_unlimited');
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT c.tenant, c.owner_role, p FROM ctx c, perms
UNION ALL SELECT c.tenant, c.manager_role, p FROM ctx c, perms WHERE p NOT IN ('payroll.read', 'payroll.close', 'staff.manage', 'report.long_range', 'record.void_closed_day', 'sale.discount_unlimited', 'appointment.settings')
UNION ALL SELECT c.tenant, c.tech_role, p FROM ctx c, perms WHERE p IN ('staff.read_own', 'customer.health_notes.read');

INSERT INTO app.user_account (id, phone_e164, full_name, password_hash)
SELECT owner_user, '+84905100001', 'Chủ Spa Bench', '$argon2id$v=19$m=19456,t=2,p=1$bench$bench' FROM ctx
UNION ALL SELECT manager_user, '+84905100002', 'Quản Lý Bench', '$argon2id$v=19$m=19456,t=2,p=1$bench$bench' FROM ctx
UNION ALL SELECT tech_user, '+84905100003', 'Kỹ Thuật Bench', '$argon2id$v=19$m=19456,t=2,p=1$bench$bench' FROM ctx;

CREATE TEMP TABLE mem AS
SELECT gen_random_uuid() AS id, u, r FROM ctx, LATERAL (VALUES (owner_user, owner_role), (manager_user, manager_role), (tech_user, tech_role)) v(u, r);
INSERT INTO app.membership (id, tenant_id, user_id) SELECT m.id, c.tenant, m.u FROM mem m, ctx c;
INSERT INTO app.membership_role (tenant_id, membership_id, role_id) SELECT c.tenant, m.id, m.r FROM mem m, ctx c;

-- 10 staff: owner, manager, 8 technicians (the first technician has the tech login).
CREATE TEMP TABLE st AS
SELECT g AS n, gen_random_uuid() AS id FROM generate_series(1, 10) g;
INSERT INTO app.staff (id, tenant_id, branch_id, full_name, phone_e164, position, joined_on, user_id, shift_start)
SELECT s.id, c.tenant, c.branch,
       (ARRAY['Chủ Spa Bench', 'Quản Lý Bench', 'Nguyễn Thị Lan', 'Trần Thị Mai', 'Lê Thị Hoa', 'Phạm Thị Hương',
              'Hoàng Thị Linh', 'Võ Thị Trang', 'Đặng Thị Yến', 'Bùi Thị Nhung'])[s.n],
       '+8490520000' || (s.n - 1),
       CASE s.n WHEN 1 THEN 'Chủ spa' WHEN 2 THEN 'Quản lý' ELSE 'Kỹ thuật viên' END,
       c.first_day,
       CASE s.n WHEN 1 THEN c.owner_user WHEN 2 THEN c.manager_user WHEN 3 THEN c.tech_user END,
       time '08:30'
FROM st s, ctx c;
INSERT INTO app.staff_pay (tenant_id, staff_id, base_salary, commission_rate_bp, loyalty_bonus_monthly, meal_allowance)
SELECT c.tenant, s.id, 6000000, 1000, 300000, 600000 FROM st s, ctx c WHERE s.n >= 3;
CREATE TEMP TABLE tech AS SELECT row_number() OVER (ORDER BY n) - 1 AS k, id FROM st WHERE n >= 3;

-- ------------------------------------------------------------------ catalog
CREATE TEMP TABLE cat AS SELECT g AS n, gen_random_uuid() AS id FROM generate_series(1, 10) g;
INSERT INTO app.service_category (id, tenant_id, name, sort_order)
SELECT k.id, c.tenant, 'Nhóm ' || k.n, k.n FROM cat k, ctx c;
CREATE TEMP TABLE svc AS
SELECT g AS n, gen_random_uuid() AS id, 150000 + (g % 8) * 50000 AS price, 30 + (g % 4) * 15 AS minutes
FROM generate_series(1, 35) g;
INSERT INTO app.service (id, tenant_id, category_id, name, duration_min, sort_order)
SELECT s.id, c.tenant, k.id, 'Dịch vụ ' || s.n, s.minutes, s.n
FROM svc s JOIN cat k ON k.n = 1 + (s.n % 10), ctx c;
INSERT INTO app.service_price (tenant_id, service_id, variant, amount)
SELECT c.tenant, s.id, 'all', s.price FROM svc s, ctx c WHERE s.n > 10
UNION ALL SELECT c.tenant, s.id, v, s.price + CASE v WHEN 'male' THEN 50000 ELSE 0 END
FROM svc s, ctx c, unnest(ARRAY['female', 'male']) v WHERE s.n <= 10;

CREATE TEMP TABLE plan AS
SELECT g AS n, gen_random_uuid() AS id, 1000000 + g * 400000 AS price, (SELECT id FROM svc WHERE n = 10 + g) AS service
FROM generate_series(1, 6) g;
INSERT INTO app.package_plan (id, tenant_id, name, price, validity_months)
SELECT p.id, c.tenant, 'Gói ' || p.n || ' (10 + 2 buổi)', p.price, CASE WHEN p.n % 2 = 0 THEN 12 END FROM plan p, ctx c;
INSERT INTO app.package_plan_item (id, tenant_id, plan_id, service_id, sessions, bonus_sessions)
SELECT gen_random_uuid(), c.tenant, p.id, p.service, 10, 2 FROM plan p, ctx c;

-- ------------------------------------------------------------------ customers
CREATE TEMP TABLE cust AS
SELECT g AS n, gen_random_uuid() AS id,
       CASE WHEN g % 20 = 0 THEN NULL ELSE
         (ARRAY['Nguyễn','Trần','Lê','Phạm','Hoàng','Huỳnh','Phan','Vũ','Võ','Đặng','Bùi','Đỗ','Hồ','Ngô','Dương','Lý'])[1 + g % 16] || ' ' ||
         (ARRAY['Thị','Văn','Ngọc','Minh','Thanh','Thu','Hồng','Kim'])[1 + (g / 16) % 8] || ' ' ||
         (ARRAY['Lan','Hoa','Mai','Hương','Linh','Trang','Anh','Hà','Nga','Thảo','Hằng','Yến','Vy','Nhung','Phương','Dung','Tâm','Hạnh','Loan','Tuyết'])[1 + (g / 128) % 20]
       END AS full_name
FROM generate_series(1, :customers) g;
CREATE INDEX ON cust (n);
INSERT INTO app.customer (id, tenant_id, full_name, search_text, phone_e164, gender, birth_year, note, health_note_ct, created_at, updated_at)
SELECT k.id, c.tenant, k.full_name, coalesce(app.search_fold(k.full_name), ''),
       CASE WHEN k.n % 25 = 0 THEN NULL ELSE '+8491' || lpad(k.n::text, 7, '0') END,
       CASE WHEN k.n % 9 = 0 THEN 'male' ELSE 'female' END,
       1960 + k.n % 45,
       CASE WHEN k.n % 10 = 0 THEN 'Khách quen, thích phòng yên tĩnh' END,
       CASE WHEN k.n % 20 = 1 THEN decode(md5(k.n::text) || md5('a' || k.n) || md5('b' || k.n) || md5('c' || k.n) || md5('d' || k.n), 'hex') END,
       c.first_day + (k.n % :days), c.first_day + (k.n % :days)
FROM cust k, ctx c;
ANALYZE app.customer;

-- ------------------------------------------------------------------ package sales: 3 a day
CREATE TEMP TABLE ps AS
SELECT row_number() OVER (ORDER BY d, k) - 1 AS idx, d::date AS day, k,
       gen_random_uuid() AS sale, gen_random_uuid() AS pkg, gen_random_uuid() AS item,
       (SELECT id FROM cust WHERE n = 1 + abs(hashtext(d::text || 'p' || k)) % :customers AND full_name IS NOT NULL) AS customer,
       1 + abs(hashtext(d::text || 'plan' || k)) % 6 AS plan_n,
       abs(hashtext(d::text || 'closer' || k)) % 8 AS closer_k
FROM generate_series((SELECT first_day FROM ctx), current_date, interval '1 day') d
CROSS JOIN generate_series(1, 3) k;
DELETE FROM ps WHERE customer IS NULL;  -- the walk-in profiles cannot own a package
CREATE INDEX ON ps (idx);
INSERT INTO app.sale (id, tenant_id, branch_id, customer_id, business_date, subtotal, total, amount_paid, closer_staff_id, created_by, created_at, note_ct)
SELECT p.sale, c.tenant, c.branch, p.customer, p.day, pl.price, pl.price, pl.price, t.id, c.manager_user,
       (p.day + time '10:00' + p.k * interval '1 hour') AT TIME ZONE 'Asia/Ho_Chi_Minh',
       CASE WHEN p.idx % 4 = 0 THEN decode(md5(p.idx::text) || md5('x' || p.idx) || md5('y' || p.idx), 'hex') END
FROM ps p JOIN plan pl ON pl.n = p.plan_n JOIN tech t ON t.k = p.closer_k, ctx c;
ANALYZE app.sale;
INSERT INTO app.customer_package (id, tenant_id, customer_id, plan_id, name_snapshot, sold_price, origin, sale_id, closer_staff_id, starts_on, expires_on, created_by, created_at)
SELECT p.pkg, c.tenant, p.customer, pl.id, 'Gói ' || pl.n || ' (10 + 2 buổi)', pl.price, 'sale', p.sale, t.id, p.day,
       CASE WHEN pl.n % 2 = 0 THEN (p.day + interval '12 months')::date END, c.manager_user,
       (p.day + time '10:00' + p.k * interval '1 hour') AT TIME ZONE 'Asia/Ho_Chi_Minh'
FROM ps p JOIN plan pl ON pl.n = p.plan_n JOIN tech t ON t.k = p.closer_k, ctx c;
ANALYZE app.customer_package;
INSERT INTO app.customer_package_item (id, tenant_id, customer_package_id, service_id, name_snapshot, sessions_purchased, sessions_bonus, sessions_remaining, per_session_value)
SELECT p.item, c.tenant, p.pkg, pl.service, 'Dịch vụ ' || (10 + pl.n), 10, 2,
       greatest(0, 12 - ((current_date - p.day) / 14)), pl.price / 10
FROM ps p JOIN plan pl ON pl.n = p.plan_n, ctx c;
ANALYZE app.customer_package_item;
INSERT INTO app.sale_line (id, tenant_id, sale_id, line_type, package_plan_id, customer_package_id, description_snapshot, qty, unit_price, line_total, position)
SELECT gen_random_uuid(), c.tenant, p.sale, 'package', pl.id, p.pkg, 'Gói ' || pl.n, 1, pl.price, pl.price, 0
FROM ps p JOIN plan pl ON pl.n = p.plan_n, ctx c;
INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, method, amount, created_by, paid_at)
SELECT gen_random_uuid(), c.tenant, c.branch, p.sale, p.day, CASE WHEN p.idx % 10 < 7 THEN 'bank_transfer' ELSE 'cash' END,
       pl.price, c.manager_user, (p.day + time '10:00' + p.k * interval '1 hour') AT TIME ZONE 'Asia/Ho_Chi_Minh'
FROM ps p JOIN plan pl ON pl.n = p.plan_n, ctx c;
ANALYZE app.payment;
-- 3 % of packages cancelled later; two thirds of those refunded.
UPDATE app.customer_package cp SET cancelled_at = cp.created_at + interval '30 days', cancel_reason = 'Khách chuyển nhà',
       cancelled_by = (SELECT owner_user FROM ctx)
FROM ps p WHERE p.pkg = cp.id AND p.idx % 33 = 5 AND p.day < current_date - 40;
UPDATE app.customer_package_item i SET sessions_remaining = 0
FROM ps p WHERE p.item = i.id AND p.idx % 33 = 5 AND p.day < current_date - 40;
INSERT INTO app.refund (id, tenant_id, branch_id, customer_id, customer_package_id, business_date, method, amount, reason, created_by, created_at)
SELECT gen_random_uuid(), c.tenant, c.branch, p.customer, p.pkg, p.day + 30, 'cash', pl.price / 2, 'Khách chuyển nhà', c.owner_user,
       (p.day + 30 + time '15:00') AT TIME ZONE 'Asia/Ho_Chi_Minh'
FROM ps p JOIN plan pl ON pl.n = p.plan_n, ctx c WHERE p.idx % 33 = 5 AND p.idx % 3 <> 0 AND p.day < current_date - 40;

-- ------------------------------------------------------------------ visits: 30–80 a day
CREATE TEMP TABLE v AS
SELECT d::date AS day, k, (d::date - (SELECT first_day FROM ctx)) AS day_idx,
       gen_random_uuid() AS visit, gen_random_uuid() AS sale,
       random() AS r_kind, random() AS r_lines, random() AS r_void, random() AS r_note, random() AS r_disc,
       ((d::date + time '08:00' + k * interval '10 minutes') AT TIME ZONE 'Asia/Ho_Chi_Minh') AS at
FROM generate_series((SELECT first_day FROM ctx), current_date, interval '1 day') d
CROSS JOIN generate_series(1, 80) k
WHERE k <= 30 + abs(hashtext(d::text || 'n')) % 51;
ALTER TABLE v ADD COLUMN pkg_idx bigint, ADD COLUMN customer uuid, ADD COLUMN lines int;
-- 35 % package visits: a package sold within the 180 days before.
UPDATE v SET pkg_idx = greatest(0, day_idx * 3 - 1 - floor(r_kind / 0.35 * least(540, day_idx * 3)))::bigint
WHERE r_kind < 0.35 AND day_idx > 0;
UPDATE v SET customer = p.customer FROM ps p WHERE p.idx = v.pkg_idx;
UPDATE v SET pkg_idx = NULL WHERE pkg_idx IS NOT NULL AND customer IS NULL;
UPDATE v SET customer = (SELECT id FROM cust WHERE n = 1 + abs(hashtext(v.day::text || 'c' || v.k)) % :customers)
WHERE customer IS NULL;
ANALYZE ps; ANALYZE cust;
UPDATE v SET lines = CASE WHEN pkg_idx IS NOT NULL THEN 1 WHEN r_lines < 0.3 THEN 2 ELSE 1 END;

CREATE TEMP TABLE vl AS
SELECT v.visit, v.sale, v.pkg_idx, v.day, v.at, v.r_void, v.r_disc, l AS position, gen_random_uuid() AS id,
       (SELECT id FROM svc WHERE n = 1 + abs(hashtext(v.visit::text || l)) % 35) AS service,
       (SELECT price FROM svc WHERE n = 1 + abs(hashtext(v.visit::text || l)) % 35) AS price,
       (SELECT n FROM svc WHERE n = 1 + abs(hashtext(v.visit::text || l)) % 35) AS service_n,
       (SELECT id FROM tech WHERE k = abs(hashtext(v.visit::text || 's' || l)) % 8) AS staff,
       CASE WHEN abs(hashtext(v.visit::text || 'ref' || l)) % 20 = 0 THEN (SELECT id FROM tech WHERE k = abs(hashtext(v.visit::text || 'r')) % 8) END AS referrer
FROM v CROSS JOIN generate_series(0, 1) l WHERE l < v.lines;
CREATE INDEX ON vl (visit);

ANALYZE v; ANALYZE vl;
-- One-off visits: their sale first (sale.visit_id is DEFERRABLE), then the visit, then the lines.
CREATE TEMP TABLE vs AS
SELECT v.visit, v.sale, v.day, v.at, v.customer, v.r_void < 0.02 AS voided,
       sum(l.price) AS subtotal,
       CASE WHEN v.r_disc < 0.05 THEN (sum(l.price) / 10) ELSE 0 END AS discount
FROM v JOIN vl l ON l.visit = v.visit WHERE v.pkg_idx IS NULL GROUP BY v.visit, v.sale, v.day, v.at, v.customer, v.r_void, v.r_disc;
INSERT INTO app.sale (id, tenant_id, branch_id, customer_id, business_date, subtotal, discount_amount, discount_reason, total, amount_paid,
                      status, void_reason, voided_at, voided_by, created_by, created_at, visit_id)
SELECT s.sale, c.tenant, c.branch, s.customer, s.day, s.subtotal, s.discount, CASE WHEN s.discount > 0 THEN 'Khách quen' END,
       s.subtotal - s.discount, CASE WHEN s.voided THEN 0 ELSE s.subtotal - s.discount END,
       CASE WHEN s.voided THEN 'voided' ELSE 'open' END, CASE WHEN s.voided THEN 'Ghi nhầm' END,
       CASE WHEN s.voided THEN s.at + interval '1 hour' END, CASE WHEN s.voided THEN c.manager_user END,
       c.manager_user, s.at, s.visit
FROM vs s, ctx c;
ANALYZE app.sale;
INSERT INTO app.visit (id, tenant_id, branch_id, customer_id, business_date, check_in_at, note_ct, status, void_reason, voided_at, voided_by, sale_id, created_by, created_at, updated_at)
SELECT v.visit, c.tenant, c.branch, v.customer, v.day, v.at,
       CASE WHEN v.r_note < 0.3 THEN decode(md5(v.visit::text) || md5('n' || v.visit) || md5('o' || v.visit) || md5('p' || v.visit) || md5('q' || v.visit), 'hex') END,
       CASE WHEN v.r_void < 0.02 THEN 'voided' ELSE 'completed' END, CASE WHEN v.r_void < 0.02 THEN 'Ghi nhầm' END,
       CASE WHEN v.r_void < 0.02 THEN v.at + interval '1 hour' END, CASE WHEN v.r_void < 0.02 THEN c.manager_user END,
       CASE WHEN v.pkg_idx IS NULL THEN v.sale END, c.manager_user, v.at, v.at
FROM v, ctx c;
ANALYZE app.visit;
INSERT INTO app.sale_line (id, tenant_id, sale_id, line_type, service_id, description_snapshot, qty, unit_price, line_total, position)
SELECT gen_random_uuid(), c.tenant, l.sale, 'service', l.service, 'Dịch vụ ' || l.service_n, 1, l.price, l.price, l.position
FROM vl l, ctx c WHERE l.pkg_idx IS NULL;
ANALYZE app.sale_line;
INSERT INTO app.visit_line (id, tenant_id, visit_id, position, service_id, service_name_snapshot, source, customer_package_id, customer_package_item_id,
                            sessions_left_after, staff_id, qty, price_variant, unit_price, line_value, discount_amount, sale_id, referrer_staff_id)
SELECT l.id, c.tenant, l.visit, l.position, coalesce(pl.service, l.service), 'Dịch vụ ' || coalesce(10 + pl.n, l.service_n),
       CASE WHEN l.pkg_idx IS NULL THEN 'one_off' ELSE 'package' END, p.pkg, p.item,
       CASE WHEN l.pkg_idx IS NOT NULL THEN 5 END, l.staff, 1, CASE WHEN l.pkg_idx IS NULL THEN 'all' END,
       coalesce(pl.price / 10, l.price),
       CASE WHEN l.pkg_idx IS NOT NULL THEN pl.price / 10
            WHEN l.position = 0 THEN l.price - coalesce(s.discount, 0) ELSE l.price END,
       CASE WHEN l.pkg_idx IS NULL AND l.position = 0 THEN coalesce(s.discount, 0) ELSE 0 END,
       CASE WHEN l.pkg_idx IS NULL THEN l.sale END,
       CASE WHEN l.pkg_idx IS NULL THEN l.referrer END
FROM vl l LEFT JOIN ps p ON p.idx = l.pkg_idx LEFT JOIN plan pl ON pl.n = p.plan_n LEFT JOIN vs s ON s.visit = l.visit, ctx c;
ANALYZE app.visit_line;
INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, method, amount, created_by, paid_at)
SELECT gen_random_uuid(), c.tenant, c.branch, s.sale, s.day, CASE WHEN abs(hashtext(s.sale::text)) % 10 < 6 THEN 'cash' ELSE 'bank_transfer' END,
       s.subtotal - s.discount, c.manager_user, s.at
FROM vs s, ctx c WHERE s.subtotal - s.discount > 0;
ANALYZE app.payment;
INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, kind, method, amount, reversal_of, created_by, paid_at)
SELECT gen_random_uuid(), p.tenant_id, p.branch_id, p.sale_id, p.business_date, 'reversal', p.method, p.amount, p.id, p.created_by, p.paid_at + interval '1 hour'
FROM app.payment p JOIN vs s ON s.sale = p.sale_id WHERE s.voided;

-- ------------------------------------------------------------------ appointments
-- Past: the first 40 visits of each day came from a booking (done); 3 cancelled + 2 no-shows a day.
CREATE TEMP TABLE ap AS
SELECT gen_random_uuid() AS id, v.day, v.at AS starts_at, v.customer, l.staff, 'done'::text AS status, v.visit
FROM v JOIN vl l ON l.visit = v.visit AND l.position = 0 WHERE v.k <= 40 AND v.r_void >= 0.02 AND v.day < current_date
UNION ALL
SELECT gen_random_uuid(), d::date, (d::date + time '09:00' + k * interval '2 hours') AT TIME ZONE 'Asia/Ho_Chi_Minh',
       (SELECT id FROM cust WHERE n = 1 + abs(hashtext(d::text || 'ap' || k)) % :customers), NULL,
       CASE WHEN k <= 3 THEN 'cancelled' ELSE 'no_show' END, NULL
FROM generate_series((SELECT first_day FROM ctx), current_date - 1, interval '1 day') d CROSS JOIN generate_series(1, 5) k
UNION ALL
-- Future: 20 bookings a day for two weeks, two 60-minute rounds per technician (no overlap).
SELECT gen_random_uuid(), d::date, (d::date + time '09:00' + (k / 8) * interval '90 minutes') AT TIME ZONE 'Asia/Ho_Chi_Minh',
       (SELECT id FROM cust WHERE n = 1 + abs(hashtext(d::text || 'fb' || k)) % :customers),
       (SELECT id FROM tech WHERE tech.k = ap_k.k % 8), 'booked', NULL
FROM generate_series(current_date, current_date + 13, interval '1 day') d CROSS JOIN generate_series(0, 15) ap_k(k);
INSERT INTO app.appointment (id, tenant_id, branch_id, customer_id, business_date, starts_at, ends_at, staff_id, status, note, status_reason,
                             closed_at, closed_by, visit_id, created_by, created_at, updated_at)
SELECT a.id, c.tenant, c.branch, a.customer, a.day, a.starts_at, a.starts_at + interval '60 minutes', a.staff, a.status,
       CASE WHEN abs(hashtext(a.id::text)) % 10 = 0 THEN 'Khách muốn KTV quen' END,
       CASE WHEN a.status IN ('cancelled', 'no_show') THEN 'Khách bận' END,
       CASE WHEN a.status <> 'booked' THEN a.starts_at + interval '70 minutes' END,
       CASE WHEN a.status <> 'booked' THEN c.manager_user END,
       a.visit, c.manager_user, a.starts_at - interval '2 days', a.starts_at
FROM ap a, ctx c;
ANALYZE app.appointment;
INSERT INTO app.appointment_service (tenant_id, appointment_id, position, service_id, service_name_snapshot)
SELECT c.tenant, a.id, 0, s.id, 'Dịch vụ ' || s.n FROM ap a JOIN svc s ON s.n = 1 + abs(hashtext(a.id::text)) % 35, ctx c;

-- ------------------------------------------------------------------ staff books (before periods close)
INSERT INTO app.attendance_day (tenant_id, staff_id, work_date, status, check_in, check_out, late_minutes, note, recorded_by)
SELECT c.tenant, s.id, d::date,
       CASE WHEN abs(hashtext(s.id::text || d::text)) % 100 < 92 THEN 'present' WHEN abs(hashtext(s.id::text || d::text)) % 2 = 0 THEN 'leave_p' ELSE 'absent_k' END,
       CASE WHEN abs(hashtext(s.id::text || d::text)) % 100 < 92 THEN time '08:30' + (abs(hashtext(d::text || s.id::text)) % 20) * interval '1 minute' END,
       CASE WHEN abs(hashtext(s.id::text || d::text)) % 100 < 92 THEN time '20:30' END,
       CASE WHEN abs(hashtext(s.id::text || d::text)) % 100 < 92 THEN greatest(0, abs(hashtext(d::text || s.id::text)) % 20 - 10) ELSE 0 END,
       CASE WHEN abs(hashtext(s.id::text || d::text)) % 30 = 0 THEN 'Đi muộn vì kẹt xe' END,
       c.manager_user
FROM st s, ctx c, generate_series((SELECT first_day FROM ctx), current_date - 1, interval '1 day') d WHERE s.n >= 2;
ANALYZE app.attendance_day;
INSERT INTO app.staff_daily_review (tenant_id, staff_id, work_date, compliments, complaints, note, recorded_by)
SELECT a.tenant_id, a.staff_id, a.work_date, abs(hashtext(a.staff_id::text || a.work_date)) % 4, (abs(hashtext(a.work_date || a.staff_id::text)) % 10 = 0)::int,
       CASE WHEN abs(hashtext(a.staff_id::text || a.work_date)) % 15 = 0 THEN 'Khách khen tay nghề' END, a.recorded_by
FROM app.attendance_day a WHERE a.status = 'present' AND a.staff_id IN (SELECT id FROM tech);
INSERT INTO app.staff_evaluation (tenant_id, staff_id, work_date, criteria, total_points, result_level, coaching_plan, comment, recorded_by)
SELECT c.tenant, t.id, m::date,
       jsonb_build_array(
         jsonb_build_object('key', 'attendance', 'max', 20, 'level', 'good', 'points', 18),
         jsonb_build_object('key', 'skill', 'max', 25, 'level', 'good', 'points', 22),
         jsonb_build_object('key', 'attitude', 'max', 20, 'level', 'ok', 'points', 15),
         jsonb_build_object('key', 'hygiene', 'max', 15, 'level', 'good', 'points', 14),
         jsonb_build_object('key', 'sales', 'max', 10, 'level', 'ok', 'points', 6),
         jsonb_build_object('key', 'teamwork', 'max', 10, 'level', 'good', 'points', 9)),
       84, CASE WHEN abs(hashtext(t.id::text || m::text)) % 12 = 0 THEN 'm3' ELSE 'm1' END,
       CASE WHEN abs(hashtext(t.id::text || m::text)) % 12 = 0 THEN 'Kèm cặp kỹ thuật massage 2 tuần' END,
       CASE WHEN abs(hashtext(t.id::text || m::text)) % 4 = 0 THEN 'Tiến bộ tốt' END, c.manager_user
FROM tech t, ctx c, generate_series(date_trunc('month', (SELECT first_day FROM ctx))::date + interval '1 month', date_trunc('month', current_date) - interval '1 month', interval '1 month') m;
INSERT INTO app.staff_adjustment (id, tenant_id, staff_id, work_date, kind, amount, points, reason, created_by)
SELECT gen_random_uuid(), c.tenant, t.id, (m + interval '14 days')::date, CASE WHEN j = 1 THEN 'bonus' ELSE 'penalty' END,
       CASE WHEN j = 1 THEN 200000 ELSE 50000 END, CASE WHEN j = 2 THEN 2 END, 'Ghi chú thưởng/phạt', c.manager_user
FROM tech t, ctx c, generate_series(date_trunc('month', (SELECT first_day FROM ctx))::date + interval '1 month', date_trunc('month', current_date), interval '1 month') m,
     generate_series(1, 2) j WHERE abs(hashtext(t.id::text || m::text || j)) % 3 = 0;
INSERT INTO app.payroll_period (tenant_id, period, status, closed_at, closed_by)
SELECT c.tenant, m::date, 'closed', m + interval '1 month 3 days', c.owner_user
FROM ctx c, generate_series(date_trunc('month', (SELECT first_day FROM ctx))::date + interval '1 month', date_trunc('month', current_date) - interval '2 months', interval '1 month') m;
INSERT INTO app.payroll_period (tenant_id, period) SELECT tenant, date_trunc('month', current_date - interval '1 month')::date FROM ctx;
INSERT INTO app.payslip (id, tenant_id, period, staff_id, net_pay, detail, created_at)
SELECT gen_random_uuid(), p.tenant_id, p.period, t.id, 9500000,
       jsonb_build_object('base_salary', 6000000, 'days_worked', 28, 'commission', jsonb_build_object('rate_bp', 1000, 'base', 25000000, 'amount', 2500000,
         'lines', (SELECT jsonb_agg(jsonb_build_object('kind', 'service', 'name', 'Dịch vụ ' || g, 'qty', g, 'value', g * 150000)) FROM generate_series(1, 12) g)),
         'loyalty_bonus', 300000, 'meal_allowance', 600000, 'adjustments', '[]'::jsonb, 'evaluation', jsonb_build_object('points', 84, 'level', 'm1')),
       p.closed_at
FROM app.payroll_period p CROSS JOIN tech t WHERE p.status = 'closed';

-- ------------------------------------------------------------------ CRM extras
INSERT INTO app.customer_follow_up (id, tenant_id, customer_id, contacted_on, channel, outcome, note, next_follow_up_on, created_by, created_at)
SELECT gen_random_uuid(), c.tenant, (SELECT id FROM cust WHERE n = 1 + abs(hashtext(d::text || 'f' || k)) % :customers), d::date,
       'zalo', CASE WHEN k = 1 THEN 'will_come' ELSE 'no_answer' END, CASE WHEN k = 1 THEN 'Hẹn tuần sau' END,
       d::date + CASE WHEN k = 1 THEN 7 ELSE 3 END, c.manager_user, (d::date + time '16:00') AT TIME ZONE 'Asia/Ho_Chi_Minh'
FROM ctx c, generate_series((SELECT first_day FROM ctx), current_date, interval '1 day') d CROSS JOIN generate_series(1, 3) k;

-- ------------------------------------------------------------------ audit (one row per mutation, sizes like staging)
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, after, at)
SELECT gen_random_uuid(), c.tenant, c.manager_user, 'user', 'visit.create', 'visit', v.visit,
       jsonb_build_object('customer_id', v.customer, 'business_date', v.day, 'check_in_at', v.at, 'lines',
         (SELECT jsonb_agg(jsonb_build_object('service_id', l.service, 'staff_id', l.staff, 'source', CASE WHEN l.pkg_idx IS NULL THEN 'one_off' ELSE 'package' END, 'qty', 1, 'unit_price', l.price, 'line_value', l.price)) FROM vl l WHERE l.visit = v.visit),
         'sale_id', v.sale, 'payments', jsonb_build_array(jsonb_build_object('method', 'cash', 'amount', 150000))),
       v.at
FROM v, ctx c;
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, after, at)
SELECT gen_random_uuid(), s.tenant_id, s.created_by, 'user', 'sale.create', 'sale', s.id,
       jsonb_build_object('customer_id', s.customer_id, 'business_date', s.business_date, 'subtotal', s.subtotal, 'discount_amount', s.discount_amount,
                          'total', s.total, 'lines', jsonb_build_array(jsonb_build_object('line_type', 'service', 'qty', 1, 'unit_price', s.subtotal)),
                          'payments', jsonb_build_array(jsonb_build_object('method', 'cash', 'amount', s.total))),
       s.created_at
FROM app.sale s WHERE s.tenant_id = (SELECT tenant FROM ctx);
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, after, at)
SELECT gen_random_uuid(), a.tenant_id, a.created_by, 'user', x.action, 'appointment', a.id,
       jsonb_build_object('customer_id', a.customer_id, 'starts_at', a.starts_at, 'ends_at', a.ends_at, 'staff_id', a.staff_id, 'status', a.status, 'services', jsonb_build_array(jsonb_build_object('service_id', a.id))),
       a.created_at
FROM app.appointment a CROSS JOIN (VALUES ('appointment.create'), ('appointment.complete')) x(action)
WHERE a.tenant_id = (SELECT tenant FROM ctx) AND (x.action = 'appointment.create' OR a.status <> 'booked');
INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, after, at)
SELECT gen_random_uuid(), a.tenant_id, a.recorded_by, 'user', 'attendance.record', 'staff', a.staff_id,
       jsonb_build_object('work_date', a.work_date, 'status', a.status, 'check_in', a.check_in, 'late_minutes', a.late_minutes),
       (a.work_date + time '21:00') AT TIME ZONE 'Asia/Ho_Chi_Minh'
FROM app.attendance_day a WHERE a.tenant_id = (SELECT tenant FROM ctx);

-- ------------------------------------------------------------------ sessions (never purged) and idempotency (last 48 h)
INSERT INTO app.session (id, token_hash, user_id, active_tenant_id, created_at, last_seen_at, idle_expires_at, absolute_expires_at, recent_auth_at, ip, user_agent, revoked_at)
SELECT gen_random_uuid(), decode(md5(u::text || d::text) || md5(d::text || u::text), 'hex'), u, c.tenant, d, d + interval '10 hours',
       d + interval '22 hours', d + interval '30 days', d, '203.0.113.7', 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130.0 Safari/537.36',
       CASE WHEN d < now() - interval '1 day' THEN d + interval '11 hours' END
FROM ctx c, unnest(ARRAY[c.owner_user, c.manager_user, c.tech_user]) u, generate_series(c.first_day::timestamptz, now(), interval '1 day') d;
INSERT INTO app.idempotency_record (tenant_id, actor_user_id, key, request_hash, response_status, response_body, created_at)
SELECT c.tenant, c.manager_user, gen_random_uuid(), decode(md5(g::text) || md5('h' || g), 'hex'), 201,
       jsonb_build_object('id', gen_random_uuid(), 'customer_id', gen_random_uuid(), 'business_date', current_date, 'lines',
         (SELECT jsonb_agg(jsonb_build_object('id', gen_random_uuid(), 'service_name', 'Dịch vụ ' || k, 'qty', 1, 'unit_price', 150000, 'staff_name', 'Nguyễn Thị Lan')) FROM generate_series(1, 6) k)),
       now() - g * interval '7 minutes'
FROM ctx c, generate_series(1, 400) g;

COMMIT;
ANALYZE;
SELECT relname, n_live_tup FROM pg_stat_user_tables WHERE schemaname = 'app' AND n_live_tup > 0 ORDER BY n_live_tup DESC;
SELECT pg_size_pretty(pg_database_size(current_database())) AS db_size;
