-- 0017: staff management — profiles linked to logins, pay terms, daily attendance, daily reviews,
-- monthly evaluations, bonuses/penalties and monthly payroll with a closing lock
-- (memory/wiki/product/specs/staff-management.md, rules: memory/wiki/product/compensation-rules.md).
-- Depends on 0002 (role_permission, user_account), 0005 (staff, sale, reject_mutation), 0007 (visit_line).
-- Money is bigint đồng. Every FK is (tenant_id, x_id) so no row can point into another spa.

-- New permissions for the existing system roles (new spas get them from
-- identity::roles::SystemRole::default_permissions). Salary visibility (payroll.read) is owner only
-- by default (spec Q-S1).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, p.permission
FROM app.role r
CROSS JOIN (VALUES ('staff.manage'), ('attendance.record'), ('evaluation.record'),
                   ('payroll.read'), ('payroll.close'), ('staff.read_own')) AS p (permission)
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, p.permission
FROM app.role r
CROSS JOIN (VALUES ('attendance.record'), ('evaluation.record'), ('staff.read_own')) AS p (permission)
WHERE r.is_system AND r.key = 'manager'
ON CONFLICT DO NOTHING;
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'staff.read_own'
FROM app.role r
WHERE r.is_system AND r.key = 'technician'
ON CONFLICT DO NOTHING;

-- ------------------------------------------------------------------------------ staff profile
-- A staff member may have a login (technician self-service: own attendance, evaluation, payslip).
-- One login maps to at most one staff member per spa.
ALTER TABLE app.staff
  ADD COLUMN user_id      uuid REFERENCES app.user_account (id),
  -- Start of the shift, for "late by N minutes" (the sheet's "Giờ vào ca"). NULL = the spa default.
  ADD COLUMN shift_start  time;
CREATE UNIQUE INDEX staff_user_idx ON app.staff (tenant_id, user_id) WHERE user_id IS NOT NULL;

-- Pay terms (sensitive: only `payroll.read` holders ever receive them). One row per staff member;
-- every change is audited, and closed payslips snapshot the values they used.
CREATE TABLE app.staff_pay (
  tenant_id              uuid NOT NULL REFERENCES app.tenant (id),
  staff_id               uuid NOT NULL,
  -- The base salary actually paid per month (lương cứng thực nhận), entered by the owner.
  base_salary            bigint NOT NULL CHECK (base_salary BETWEEN 0 AND 1000000000),
  -- Commission rate in basis points (500 = 5 %: salary option 1; 700 = option 2; 1000 = option 3).
  commission_rate_bp     integer NOT NULL CHECK (commission_rate_bp BETWEEN 0 AND 10000),
  -- Thưởng gắn bó accrued per month while in a commitment contract (0 = none). Recorded on the
  -- payslip, paid at the milestone — never part of the month's net pay.
  loyalty_bonus_monthly  bigint NOT NULL DEFAULT 0 CHECK (loyalty_bonus_monthly BETWEEN 0 AND 100000000),
  -- Hỗ trợ tiền ăn, paid every month (EM question #5: entered per person).
  meal_allowance         bigint NOT NULL DEFAULT 0 CHECK (meal_allowance BETWEEN 0 AND 100000000),
  version                bigint NOT NULL DEFAULT 1,
  updated_at             timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, staff_id),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id)
);

-- --------------------------------------------------------------------------- payroll rules
-- The spa's configurable rules for the open contradictions (compensation-rules.md ⚠️ 1–8).
-- Validated and defaulted by hr::rules; a missing row means "all defaults".
CREATE TABLE app.payroll_rules (
  tenant_id   uuid PRIMARY KEY REFERENCES app.tenant (id),
  rules       jsonb NOT NULL CHECK (jsonb_typeof(rules) = 'object'),
  version     bigint NOT NULL DEFAULT 1,
  updated_by  uuid NOT NULL REFERENCES app.user_account (id),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

-- -------------------------------------------------------------------------- payroll periods
-- One row per spa and month once anything is recorded in it. 'closed' locks the month: its
-- attendance, reviews, evaluations and adjustments become read-only (service + trigger below)
-- and its payslips are frozen snapshots. Never reopened: corrections go into a later month.
CREATE TABLE app.payroll_period (
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  period      date NOT NULL CHECK (extract(day FROM period) = 1),
  status      text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
  closed_at   timestamptz,
  closed_by   uuid REFERENCES app.user_account (id),
  PRIMARY KEY (tenant_id, period),
  CHECK ((status = 'closed') = (closed_at IS NOT NULL AND closed_by IS NOT NULL))
);

-- A closed month never reopens and its close record never changes.
CREATE FUNCTION app.payroll_period_forward_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status = 'closed' OR NEW.period <> OLD.period OR NEW.tenant_id <> OLD.tenant_id THEN
    RAISE EXCEPTION 'a closed payroll period is final' USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER payroll_period_forward_only BEFORE UPDATE ON app.payroll_period
  FOR EACH ROW EXECUTE FUNCTION app.payroll_period_forward_only();
CREATE TRIGGER payroll_period_no_delete BEFORE DELETE ON app.payroll_period
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

-- Rejects writes into a closed month (defence in depth: services check first, under a row lock).
CREATE FUNCTION app.reject_closed_period() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  d date;
BEGIN
  IF TG_OP = 'DELETE' THEN
    d := OLD.work_date;
  ELSE
    d := NEW.work_date;
  END IF;
  IF TG_OP = 'UPDATE' AND OLD.work_date <> NEW.work_date THEN
    RAISE EXCEPTION 'work_date is immutable' USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF EXISTS (SELECT 1 FROM app.payroll_period p
             WHERE p.tenant_id = COALESCE(NEW.tenant_id, OLD.tenant_id)
               AND p.period = date_trunc('month', d)::date AND p.status = 'closed') THEN
    RAISE EXCEPTION 'payroll period % is closed', date_trunc('month', d)::date
      USING ERRCODE = 'insufficient_privilege';
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END
$$;

-- ------------------------------------------------------------------------------- attendance
-- One row per staff member and day (the sheet's daily line). No row = not recorded yet.
-- Codes: present (✓), leave_p (P: báo trước ≥ 3 ngày), leave_dx (ĐX: đột xuất), absent_k (K).
CREATE TABLE app.attendance_day (
  tenant_id     uuid NOT NULL REFERENCES app.tenant (id),
  staff_id      uuid NOT NULL,
  work_date     date NOT NULL,
  status        text NOT NULL CHECK (status IN ('present', 'leave_p', 'leave_dx', 'absent_k')),
  check_in      time,
  check_out     time,
  -- Minutes after the shift start, computed by the server when recorded (sheet column G).
  late_minutes  integer NOT NULL DEFAULT 0 CHECK (late_minutes BETWEEN 0 AND 1440),
  note          text CHECK (note IS NULL OR length(btrim(note)) BETWEEN 1 AND 500),
  recorded_by   uuid NOT NULL REFERENCES app.user_account (id),
  updated_at    timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, staff_id, work_date),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (status = 'present' OR (check_in IS NULL AND check_out IS NULL AND late_minutes = 0))
);
CREATE INDEX attendance_day_month_idx ON app.attendance_day (tenant_id, work_date);
CREATE TRIGGER attendance_day_closed_period BEFORE INSERT OR UPDATE OR DELETE ON app.attendance_day
  FOR EACH ROW EXECUTE FUNCTION app.reject_closed_period();

-- ----------------------------------------------------------------------------- daily review
-- The sheet's daily feedback columns (Khen / Phàn nàn / Ghi chú): the daily part of the
-- evaluation, recorded by evaluation.record holders.
CREATE TABLE app.staff_daily_review (
  tenant_id    uuid NOT NULL REFERENCES app.tenant (id),
  staff_id     uuid NOT NULL,
  work_date    date NOT NULL,
  compliments  integer NOT NULL DEFAULT 0 CHECK (compliments BETWEEN 0 AND 99),
  complaints   integer NOT NULL DEFAULT 0 CHECK (complaints BETWEEN 0 AND 99),
  note         text CHECK (note IS NULL OR length(btrim(note)) BETWEEN 1 AND 500),
  recorded_by  uuid NOT NULL REFERENCES app.user_account (id),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, staff_id, work_date),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id)
);
CREATE INDEX staff_daily_review_month_idx ON app.staff_daily_review (tenant_id, work_date);
CREATE TRIGGER staff_daily_review_closed_period BEFORE INSERT OR UPDATE OR DELETE ON app.staff_daily_review
  FOR EACH ROW EXECUTE FUNCTION app.reject_closed_period();

-- ------------------------------------------------------------------------- monthly evaluation
-- The 100-point scorecard (6 criteria), done on days 1–5 of the next month. `work_date` is the
-- first day of the evaluated month. `criteria` = [{key, max, level, points}] as scored, so a later
-- change of criteria never rewrites history. total/result are computed by hr::evaluation.
CREATE TABLE app.staff_evaluation (
  tenant_id      uuid NOT NULL REFERENCES app.tenant (id),
  staff_id       uuid NOT NULL,
  work_date      date NOT NULL CHECK (extract(day FROM work_date) = 1),
  criteria       jsonb NOT NULL CHECK (jsonb_typeof(criteria) = 'array'),
  serious_issue  boolean NOT NULL DEFAULT false,
  total_points   integer NOT NULL CHECK (total_points BETWEEN 0 AND 100),
  result_level   text NOT NULL CHECK (result_level IN ('m1', 'm2', 'm3', 'fail')),
  coaching_plan  text CHECK (coaching_plan IS NULL OR length(btrim(coaching_plan)) BETWEEN 1 AND 1000),
  comment        text CHECK (comment IS NULL OR length(btrim(comment)) BETWEEN 1 AND 1000),
  recorded_by    uuid NOT NULL REFERENCES app.user_account (id),
  version        bigint NOT NULL DEFAULT 1,
  updated_at     timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, staff_id, work_date),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (result_level IN ('m1', 'm2') OR coaching_plan IS NOT NULL)
);
CREATE TRIGGER staff_evaluation_closed_period BEFORE INSERT OR UPDATE OR DELETE ON app.staff_evaluation
  FOR EACH ROW EXECUTE FUNCTION app.reject_closed_period();

-- -------------------------------------------------------------------------- bonus / penalty
-- Append-only like the books: a wrong entry is offset by another one (opposite kind). `work_date` = the day it
-- happened (its month is the payroll month). `corrects_period` marks a correction of a closed month.
CREATE TABLE app.staff_adjustment (
  id               uuid PRIMARY KEY,
  tenant_id        uuid NOT NULL REFERENCES app.tenant (id),
  staff_id         uuid NOT NULL,
  work_date        date NOT NULL,
  kind             text NOT NULL CHECK (kind IN ('bonus', 'penalty')),
  amount           bigint CHECK (amount IS NULL OR amount BETWEEN 1 AND 100000000),
  points           integer CHECK (points IS NULL OR points BETWEEN 1 AND 100),
  reason           text NOT NULL CHECK (length(btrim(reason)) BETWEEN 1 AND 500),
  corrects_period  date CHECK (corrects_period IS NULL OR extract(day FROM corrects_period) = 1),
  created_by       uuid NOT NULL REFERENCES app.user_account (id),
  created_at       timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id),
  CHECK (amount IS NOT NULL OR points IS NOT NULL),
  CHECK (corrects_period IS NULL OR corrects_period < date_trunc('month', work_date)::date)
);
CREATE INDEX staff_adjustment_month_idx ON app.staff_adjustment (tenant_id, work_date);
CREATE TRIGGER staff_adjustment_append_only BEFORE UPDATE OR DELETE ON app.staff_adjustment
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();
CREATE TRIGGER staff_adjustment_closed_period BEFORE INSERT ON app.staff_adjustment
  FOR EACH ROW EXECUTE FUNCTION app.reject_closed_period();

-- ---------------------------------------------------------------------------------- payslips
-- Written once when a month is closed; immutable afterwards. `detail` holds the full computed
-- payslip (inputs, rules used, commission per service) exactly as shown.
CREATE TABLE app.payslip (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  period      date NOT NULL,
  staff_id    uuid NOT NULL,
  net_pay     bigint NOT NULL,
  detail      jsonb NOT NULL CHECK (jsonb_typeof(detail) = 'object'),
  created_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, period, staff_id),
  FOREIGN KEY (tenant_id, period) REFERENCES app.payroll_period (tenant_id, period),
  FOREIGN KEY (tenant_id, staff_id) REFERENCES app.staff (tenant_id, id)
);
CREATE TRIGGER payslip_append_only BEFORE UPDATE OR DELETE ON app.payslip
  FOR EACH ROW EXECUTE FUNCTION app.reject_mutation();

-- Commission inputs per technician and month (visit_line has (tenant_id, staff_id) only).
CREATE INDEX visit_day_status_idx ON app.visit (tenant_id, business_date) WHERE status = 'completed';
CREATE INDEX sale_closer_day_idx ON app.sale (tenant_id, closer_staff_id, business_date)
  WHERE closer_staff_id IS NOT NULL AND status = 'open';

-- ---------------------------------------------------------------- privileges & RLS
GRANT SELECT, INSERT, UPDATE ON app.staff_pay TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.payroll_rules TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.payroll_period TO app_rw;          -- open → closed only
GRANT SELECT, INSERT, UPDATE, DELETE ON app.attendance_day TO app_rw;  -- clearing a day = DELETE
GRANT SELECT, INSERT, UPDATE, DELETE ON app.staff_daily_review TO app_rw;
GRANT SELECT, INSERT, UPDATE ON app.staff_evaluation TO app_rw;
GRANT SELECT, INSERT ON app.staff_adjustment TO app_rw;                -- append-only
GRANT SELECT, INSERT ON app.payslip TO app_rw;                         -- append-only

ALTER TABLE app.staff_pay ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff_pay
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.payroll_rules ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.payroll_rules
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.payroll_period ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.payroll_period
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.attendance_day ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.attendance_day
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.staff_daily_review ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff_daily_review
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.staff_evaluation ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff_evaluation
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.staff_adjustment ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.staff_adjustment
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.payslip ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.payslip
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
