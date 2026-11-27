-- 0055: the owner's lucky draw, "Vòng quay may mắn" (spec product/specs/lucky-draw.md, contract
-- architecture/lucky-draw.md, ADR-0025).
--
-- Four tenant tables: a draw, its participants, its prizes and its results. Every child row
-- references its draw with the tenant (no cross-spa link can exist) and a result names a prize
-- and a participant OF THE SAME DRAW (three-column foreign keys). Deleting a draw takes its whole
-- tree with it (ON DELETE CASCADE); a prize or a participant that has a result cannot be deleted
-- on its own (RESTRICT). A result is written once by a spin and may only be forfeited, once
-- (trigger `lucky_draw_result_forfeit_only`); nothing else on it ever changes.
--
-- The winner is picked by the API under a row lock of the draw (`lucky_draw::service`): every
-- write of a draw locks its `lucky_draw` row first, so spins and edits of one draw run one after
-- the other. `version` of the draw goes up on every write inside it (the ETag of
-- `GET /lucky-draws/{id}`).
--
-- Deploy: independent of the previous binary (new tables, a new permission it does not know: a
-- spa role edit in the console may answer `422 unknown_permission` until the new binary runs).
-- Migrate first, then deploy.

-- ---------------------------------------------------------------- tables
CREATE TABLE app.lucky_draw (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 80),
  created_by  uuid NOT NULL REFERENCES app.user_account (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  version     bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  UNIQUE (tenant_id, id)
);
-- The list, newest first.
CREATE INDEX lucky_draw_tenant_created_idx ON app.lucky_draw (tenant_id, created_at DESC);

CREATE TABLE app.lucky_draw_participant (
  id          uuid PRIMARY KEY,
  tenant_id   uuid NOT NULL REFERENCES app.tenant (id),
  draw_id     uuid NOT NULL,
  name        text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 120),
  gender      text NOT NULL CHECK (gender IN ('male', 'female')),
  -- Entry order; assigned max + 1 under the draw's lock (gaps after a delete are fine).
  position    integer NOT NULL CHECK (position >= 1),
  created_by  uuid NOT NULL REFERENCES app.user_account (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  version     bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  UNIQUE (tenant_id, id),
  -- Target of the result's three-column key: a winner of this draw.
  UNIQUE (tenant_id, draw_id, id),
  -- Also the index of "the participants of a draw, in order".
  UNIQUE (tenant_id, draw_id, position),
  FOREIGN KEY (tenant_id, draw_id) REFERENCES app.lucky_draw (tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE app.lucky_draw_prize (
  id           uuid PRIMARY KEY,
  tenant_id    uuid NOT NULL REFERENCES app.tenant (id),
  draw_id      uuid NOT NULL,
  name         text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 140),
  description  text CHECK (description IS NULL OR length(btrim(description)) BETWEEN 1 AND 250),
  eligible     text NOT NULL CHECK (eligible IN ('all', 'male', 'female')),
  quantity     integer NOT NULL CHECK (quantity BETWEEN 1 AND 1000),
  position     integer NOT NULL CHECK (position >= 1),
  created_by   uuid NOT NULL REFERENCES app.user_account (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  version      bigint NOT NULL DEFAULT 1 CHECK (version >= 1),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, draw_id, id),
  UNIQUE (tenant_id, draw_id, position),
  FOREIGN KEY (tenant_id, draw_id) REFERENCES app.lucky_draw (tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE app.lucky_draw_result (
  id              uuid PRIMARY KEY,
  tenant_id       uuid NOT NULL REFERENCES app.tenant (id),
  draw_id         uuid NOT NULL,
  prize_id        uuid NOT NULL,
  participant_id  uuid NOT NULL,
  -- Spin number within the draw: max + 1 under the draw's lock.
  seq             integer NOT NULL CHECK (seq > 0),
  drawn_at        timestamptz NOT NULL DEFAULT clock_timestamp(),
  drawn_by        uuid NOT NULL REFERENCES app.user_account (id),
  -- "Vắng mặt, trả giải": the slot goes back to the prize, the person stays out of the draw.
  forfeited_at    timestamptz,
  forfeited_by    uuid REFERENCES app.user_account (id),
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, draw_id, seq),
  -- One result per person and draw, forfeited or not: nobody wins twice.
  UNIQUE (tenant_id, draw_id, participant_id),
  CONSTRAINT lucky_draw_result_forfeit_complete CHECK ((forfeited_at IS NULL) = (forfeited_by IS NULL)),
  CHECK (forfeited_at IS NULL OR forfeited_at >= drawn_at),
  FOREIGN KEY (tenant_id, draw_id) REFERENCES app.lucky_draw (tenant_id, id) ON DELETE CASCADE,
  -- RESTRICT: a prize / a person with a result stays. Deleting the draw still removes all of them
  -- in one statement: the restrict checks run after every cascade of that statement.
  FOREIGN KEY (tenant_id, draw_id, prize_id)
    REFERENCES app.lucky_draw_prize (tenant_id, draw_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, draw_id, participant_id)
    REFERENCES app.lucky_draw_participant (tenant_id, draw_id, id) ON DELETE RESTRICT
);
-- The restrict check of a prize delete, and "awarded per prize".
CREATE INDEX lucky_draw_result_prize_idx ON app.lucky_draw_result (tenant_id, draw_id, prize_id);

-- ---------------------------------------------------------------- a result is final
-- The only change a result accepts: forfeited, once (from NULL to a value; the CHECK above wants
-- both columns). A row is deleted only with its draw: a DELETE fired by the cascade runs inside
-- the foreign key's trigger (depth 2); one issued directly (depth 1) is refused, the schema
-- owner's included (`app reset-demo` disables user triggers for its transaction, as it does for
-- every append-only table).
CREATE FUNCTION app.lucky_draw_result_forfeit_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN
    IF pg_trigger_depth() < 2 THEN
      RAISE EXCEPTION 'lucky_draw_result is deleted only with its draw' USING ERRCODE = 'insufficient_privilege';
    END IF;
    RETURN OLD;
  END IF;
  IF OLD.forfeited_at IS NOT NULL
     OR NEW.forfeited_at IS NULL
     OR (NEW.id, NEW.tenant_id, NEW.draw_id, NEW.prize_id, NEW.participant_id, NEW.seq,
         NEW.drawn_at, NEW.drawn_by)
        IS DISTINCT FROM
        (OLD.id, OLD.tenant_id, OLD.draw_id, OLD.prize_id, OLD.participant_id, OLD.seq,
         OLD.drawn_at, OLD.drawn_by) THEN
    RAISE EXCEPTION 'lucky_draw_result can only be forfeited, once' USING ERRCODE = 'insufficient_privilege';
  END IF;
  RETURN NEW;
END
$$;
CREATE TRIGGER lucky_draw_result_forfeit_only BEFORE UPDATE OR DELETE ON app.lucky_draw_result
  FOR EACH ROW EXECUTE FUNCTION app.lucky_draw_result_forfeit_only();

-- ---------------------------------------------------------------- privileges & RLS
-- Only what the use cases do: rename a draw (+ version), edit a person / a prize (+ version),
-- delete a draw / a person / a prize, forfeit a result. Ids, draws and authors never change.
GRANT SELECT, INSERT, DELETE ON app.lucky_draw TO app_rw;
GRANT UPDATE (name, updated_at, version) ON app.lucky_draw TO app_rw;
GRANT SELECT, INSERT, DELETE ON app.lucky_draw_participant TO app_rw;
GRANT UPDATE (name, gender, updated_at, version) ON app.lucky_draw_participant TO app_rw;
GRANT SELECT, INSERT, DELETE ON app.lucky_draw_prize TO app_rw;
GRANT UPDATE (name, description, eligible, quantity, updated_at, version) ON app.lucky_draw_prize TO app_rw;
-- No DELETE: a result goes only with its draw (the cascade runs as the table owner).
GRANT SELECT, INSERT ON app.lucky_draw_result TO app_rw;
GRANT UPDATE (forfeited_at, forfeited_by) ON app.lucky_draw_result TO app_rw;

ALTER TABLE app.lucky_draw ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.lucky_draw
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.lucky_draw_participant ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.lucky_draw_participant
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.lucky_draw_prize ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.lucky_draw_prize
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());
ALTER TABLE app.lucky_draw_result ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.lucky_draw_result
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

-- ---------------------------------------------------------------- the permission
-- `lucky_draw.manage`: the owner's, in every spa and in the owner role template (a new spa's
-- owner gets it), like `record.history.read` in 0052. Idempotent. A spa whose owner role follows
-- the template keeps following it: role and template gain the same grant.
INSERT INTO app.role_template_permission (role_key, permission)
VALUES ('owner', 'lucky_draw.manage')
ON CONFLICT DO NOTHING;

INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'lucky_draw.manage'
FROM app.role r
WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------- daily checks
-- 0035's checks, plus two over the draws: a prize never has more results kept (not forfeited)
-- than slots, and every winner matches the prize's gender. Counts only, like the others.
CREATE OR REPLACE FUNCTION app.invariant_violations()
RETURNS TABLE (check_name text, violations bigint)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT 'sale_amount_paid', count(*) FROM app.sale s
  LEFT JOIN LATERAL (
    SELECT sum(amount) FILTER (WHERE kind = 'payment') AS paid,
           sum(amount) FILTER (WHERE kind = 'reversal') AS reversed
    FROM app.payment WHERE tenant_id = s.tenant_id AND sale_id = s.id) p ON true
  WHERE (s.status = 'open' AND s.amount_paid <> coalesce(p.paid, 0) - coalesce(p.reversed, 0))
     OR (s.status = 'voided' AND (s.amount_paid <> 0 OR coalesce(p.paid, 0) <> coalesce(p.reversed, 0)))
  UNION ALL
  SELECT 'payment_branch', count(*) FROM app.payment p
  JOIN app.sale s ON s.tenant_id = p.tenant_id AND s.id = p.sale_id WHERE p.branch_id <> s.branch_id
  UNION ALL
  SELECT 'visit_sale_link', (
    SELECT count(*) FROM app.visit v JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.id = v.sale_id
    WHERE s.visit_id IS DISTINCT FROM v.id) + (
    SELECT count(*) FROM app.visit_line l JOIN app.sale s ON s.tenant_id = l.tenant_id AND s.id = l.sale_id
    WHERE s.visit_id IS DISTINCT FROM l.visit_id)
  UNION ALL
  SELECT 'voided_visit_open_sale', count(*) FROM app.visit v
  JOIN app.sale s ON s.tenant_id = v.tenant_id AND s.visit_id = v.id
  WHERE v.status = 'voided' AND s.status = 'open'
  UNION ALL
  SELECT 'sessions_remaining', count(*) FROM app.customer_package_item i
  JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
  LEFT JOIN LATERAL (
    SELECT sum(l.qty) AS used FROM app.visit_line l
    JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
    WHERE l.tenant_id = i.tenant_id AND l.customer_package_item_id = i.id AND v.status = 'completed') u ON true
  WHERE p.origin = 'sale' AND p.cancelled_at IS NULL
    AND i.sessions_remaining <> i.sessions_total - coalesce(u.used, 0)
  UNION ALL
  SELECT 'cancelled_package_sessions', count(*) FROM app.customer_package_item i
  JOIN app.customer_package p ON p.tenant_id = i.tenant_id AND p.id = i.customer_package_id
  WHERE p.cancelled_at IS NOT NULL AND i.sessions_remaining > 0
  UNION ALL
  SELECT 'package_sale_copy', count(*) FROM (
    SELECT 1 FROM app.sale s
    JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id AND cp.origin = 'sale'
    GROUP BY s.tenant_id, s.id, s.total, s.closer_staff_id
    HAVING sum(cp.sold_price) <> s.total
        OR NOT bool_and(cp.closer_staff_id IS NOT DISTINCT FROM s.closer_staff_id)) x
  UNION ALL
  SELECT 'staff_mirrors_employment', count(*) FROM app.staff s
  JOIN LATERAL (
    SELECT started_on, ended_on FROM app.staff_employment
    WHERE tenant_id = s.tenant_id AND staff_id = s.id
    ORDER BY coalesce(started_on, '-infinity'::date) DESC, created_at DESC LIMIT 1) e ON true
  WHERE (s.status = 'left') <> (e.ended_on IS NOT NULL)
     OR s.left_on IS DISTINCT FROM e.ended_on OR s.joined_on IS DISTINCT FROM e.started_on
  UNION ALL
  SELECT 'customer_search_text', count(*) FROM app.customer
  WHERE search_text <> coalesce(app.search_fold(full_name), '')
  UNION ALL
  SELECT 'business_date_local_day', (
    SELECT count(*) FROM app.visit v JOIN app.tenant t ON t.id = v.tenant_id
    WHERE v.business_date <> (v.check_in_at AT TIME ZONE t.timezone)::date) + (
    SELECT count(*) FROM app.appointment a JOIN app.tenant t ON t.id = a.tenant_id
    WHERE a.business_date <> (a.starts_at AT TIME ZONE t.timezone)::date)
  UNION ALL
  SELECT 'appointment_visit_customer', count(*) FROM app.appointment a
  JOIN app.visit v ON v.tenant_id = a.tenant_id AND v.id = a.visit_id
  WHERE a.customer_id IS DISTINCT FROM v.customer_id
  UNION ALL
  SELECT 'lucky_draw_awarded_within_quantity', count(*) FROM app.lucky_draw_prize q
  WHERE q.quantity < (SELECT count(*) FROM app.lucky_draw_result r
                      WHERE r.tenant_id = q.tenant_id AND r.draw_id = q.draw_id
                        AND r.prize_id = q.id AND r.forfeited_at IS NULL)
  UNION ALL
  SELECT 'lucky_draw_winner_gender', count(*) FROM app.lucky_draw_result r
  JOIN app.lucky_draw_prize q ON q.tenant_id = r.tenant_id AND q.draw_id = r.draw_id AND q.id = r.prize_id
  JOIN app.lucky_draw_participant p
    ON p.tenant_id = r.tenant_id AND p.draw_id = r.draw_id AND p.id = r.participant_id
  WHERE q.eligible <> 'all' AND q.eligible <> p.gender
$$;
-- CREATE OR REPLACE keeps 0035's owner, REVOKE and GRANT.
