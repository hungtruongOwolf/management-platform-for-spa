-- 0011: security review r1 hardening. Depends on 0002 (session), 0005/0009 (packages, refunds),
-- 0006 (user_requires_mfa). 0010 is reserved for S10.

-- H1: a session row must state when the user last proved the password. With DEFAULT now(), any
-- code path that issues a token (switch-tenant rotation) silently got a fresh re-auth window.
-- Without a default, forgetting the column is a NOT NULL violation instead of a privilege grant.
ALTER TABLE app.session ALTER COLUMN recent_auth_at DROP DEFAULT;

-- M3: a suspended spa grants no permissions (identity::repo::member_permissions). MFA, though,
-- must not lapse while a spa is suspended: an owner who signed in password-only during the
-- suspension would otherwise hold a full owner session the moment the spa is reactivated.
-- So the MFA requirement looks at active memberships regardless of the spa's status.
CREATE OR REPLACE FUNCTION app.user_requires_mfa(p_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = app, pg_temp AS $$
  SELECT EXISTS (
    SELECT 1
    FROM app.membership m
    JOIN app.membership_role mr ON mr.tenant_id = m.tenant_id AND mr.membership_id = m.id
    JOIN app.role r ON r.tenant_id = mr.tenant_id AND r.id = mr.role_id
    WHERE m.user_id = p_user_id AND m.status = 'active'
      AND (r.requires_mfa OR EXISTS (
        SELECT 1 FROM app.role_permission rp
        WHERE rp.tenant_id = r.tenant_id AND rp.role_id = r.id AND rp.permission = 'platform.tenant.manage'))
  )
$$;

-- ---------------------------------------------------------------- M2: same-day paper-book import
-- EM decision 2026-11-05: "data of a day is entered that day". The owner opens the import for one
-- business day (in the spa's time zone); on any other day only the owner, with re-auth, imports.
-- One row per spa; closing deletes it. The audit log keeps who opened/closed it and when.
CREATE TABLE app.opening_balance_window (
  tenant_id  uuid PRIMARY KEY REFERENCES app.tenant (id),
  open_on    date NOT NULL,
  opened_by  uuid NOT NULL,
  opened_at  timestamptz NOT NULL DEFAULT now()
);
GRANT SELECT, INSERT, UPDATE, DELETE ON app.opening_balance_window TO app_rw;
ALTER TABLE app.opening_balance_window ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON app.opening_balance_window
  USING (tenant_id = app.current_tenant()) WITH CHECK (tenant_id = app.current_tenant());

-- The new owner-only permission, granted to every existing system owner role (new spas get it
-- from identity::roles::SystemRole::default_permissions).
INSERT INTO app.role_permission (tenant_id, role_id, permission)
SELECT r.tenant_id, r.id, 'opening_balance.manage_window'
FROM app.role r WHERE r.is_system AND r.key = 'owner'
ON CONFLICT DO NOTHING;

-- Refund cap. A sold package's cap is what was paid (sold_price). An opening balance's sold_price
-- is only what someone copied from the paper book: it caps a refund only when entered by someone
-- who may refund (the owner); otherwise the cap is 0 until the owner confirms the book price on
-- cancel. Existing opening balances start at 0 (who typed them is not provable). No default:
-- every insert states its cap.
ALTER TABLE app.customer_package ADD COLUMN refund_cap bigint;
-- The backfill must also reach cancelled packages, which customer_package_cancelled_is_final
-- (0009) protects from any UPDATE: lift that guard for this one statement only.
ALTER TABLE app.customer_package DISABLE TRIGGER customer_package_cancelled_is_final;
UPDATE app.customer_package SET refund_cap = CASE WHEN origin = 'sale' THEN sold_price ELSE 0 END;
ALTER TABLE app.customer_package ENABLE TRIGGER customer_package_cancelled_is_final;
ALTER TABLE app.customer_package
  ALTER COLUMN refund_cap SET NOT NULL,
  ADD CONSTRAINT customer_package_refund_cap_range CHECK (refund_cap BETWEEN 0 AND sold_price),
  ADD CONSTRAINT customer_package_sale_cap_is_price CHECK (origin <> 'sale' OR refund_cap = sold_price);

-- Backstop for the service check (0009), now against the cap.
CREATE OR REPLACE FUNCTION app.check_refund_within_paid() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.amount > (SELECT least(sold_price, refund_cap) FROM app.customer_package
                    WHERE tenant_id = NEW.tenant_id AND id = NEW.customer_package_id) THEN
    RAISE EXCEPTION 'refund exceeds the refund cap of package %', NEW.customer_package_id
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END
$$;
