"""Writes a psql script that EXPLAINs (ANALYZE, BUFFERS) the API's hot statements, exactly as the
API sends them: the text comes from apps/api/.sqlx (sqlx offline data) and runs as a PREPAREd
statement, as the runtime role `app_rw`, with the RLS tenant setting the API sets (db review 2026-11).

    python -I infra/perf/db/explain_hot.py > /tmp/explain.sql
    psql "$LOCAL_THROWAWAY_URL" -X -f /tmp/explain.sql      # a database seeded by seed-busy-spa.sql

Only for a local throwaway database (EXPLAIN ANALYZE executes the statement; every statement here
is a SELECT). Statements are found by the prefix of their sqlx hash: when a query changes, its hash
changes and the case prints a NOTE instead of a plan.
"""

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
SQLX = ROOT / "apps" / "api" / ".sqlx"
TYPES = {
    "Uuid": "uuid", "UuidArray": "uuid[]", "Date": "date", "Text": "text", "Int8": "int8",
    "Int4": "int4", "Bool": "bool", "Timestamptz": "timestamptz", "Bytea": "bytea",
}

# Context resolved once, as the schema owner, before switching to app_rw (psql \gset variables).
CONTEXT = r"""
\set ON_ERROR_STOP 1
\pset pager off
SELECT id AS tenant FROM app.tenant WHERE slug = 'spa-bench' \gset
SELECT id AS branch FROM app.branch WHERE tenant_id = :'tenant' \gset
SELECT user_id AS manager_user FROM app.membership m JOIN app.membership_role mr ON mr.membership_id = m.id
  JOIN app.role r ON r.id = mr.role_id WHERE m.tenant_id = :'tenant' AND r.key = 'manager' \gset
SELECT id AS tech FROM app.staff WHERE tenant_id = :'tenant' AND user_id IS NOT NULL AND position = 'Kỹ thuật viên' \gset
SELECT encode(token_hash, 'hex') AS token FROM app.session WHERE revoked_at IS NULL ORDER BY created_at DESC LIMIT 1 \gset
SELECT current_date AS today, (current_date - 29) AS d30, (current_date - 365) AS d365,
       date_trunc('month', current_date)::date AS month_start,
       (date_trunc('month', current_date) + interval '1 month - 1 day')::date AS month_end,
       date_trunc('month', current_date - interval '1 month')::date AS last_month,
       (date_trunc('month', current_date) - interval '1 day')::date AS last_month_end,
       (current_date - extract(isodow FROM current_date)::int + 1) AS monday,
       (current_date - extract(isodow FROM current_date)::int + 7) AS sunday \gset
SELECT customer_id AS busy_customer FROM app.visit WHERE tenant_id = :'tenant' GROUP BY customer_id ORDER BY count(*) DESC LIMIT 1 \gset
SELECT array_agg(id)::text AS day_visits FROM app.visit WHERE tenant_id = :'tenant' AND business_date = current_date \gset
SELECT array_agg(id)::text AS day_sales FROM app.sale WHERE tenant_id = :'tenant' AND business_date = current_date \gset
SELECT array_agg(id)::text AS week_appts FROM app.appointment WHERE tenant_id = :'tenant' AND business_date BETWEEN :'monday' AND :'sunday' \gset
SELECT array_agg(id)::text AS busy_visits FROM (SELECT id FROM app.visit WHERE tenant_id = :'tenant' AND customer_id = :'busy_customer' ORDER BY check_in_at DESC LIMIT 50) x \gset
SELECT array_agg(id)::text AS busy_pkgs FROM app.customer_package WHERE tenant_id = :'tenant' AND customer_id = :'busy_customer' \gset

-- Planner settings of the Supabase staging instance (pg_settings, 2026-11-08), so local plans
-- match what the hosted planner picks: local defaults (random_page_cost 4) favour seq scans.
SET random_page_cost = 1.1;
SET effective_cache_size = '384MB';
SET work_mem = '2184kB';
SET ROLE app_rw;
SELECT set_config('app.tenant_id', :'tenant', false) AS tenant_setting;
SET statement_timeout = '5s';
"""

T = ":'tenant'"
TZ = "'Asia/Ho_Chi_Minh'"
CASES = [
    # (label, sqlx hash prefix, arguments as SQL expressions)
    ("auth: session resolve (every request)", "f897d61cbd", ["decode(:'token', 'hex')"]),
    ("auth: member permissions (every request)", "2037edc25b", [T, ":'manager_user'"]),
    ("tenant profile (most writes)", "6b123cd425", [T]),
    ("Quầy: GET /visits?date= (visits)", "7de2ec5ce7", [T, "NULL", ":'today'"]),
    ("Quầy: GET /visits?date= (lines)", "7f78f1b307", [T, ":'day_visits'"]),
    ("Quầy: GET /visits?date= (package sales of the visits)", "26758ee3f3", [T, ":'day_visits'"]),
    ("Quầy: GET /visits?date= (payments)", "f236284941", [T, ":'day_sales'"]),
    ("Quầy: GET /sales?date= (package sales without a visit)", "ae946e9dbd", [T, ":'today'", "50"]),
    ("Lịch: GET /appointments week", "a599051908", [T, "NULL", ":'monday'", ":'sunday'"]),
    ("Lịch: appointment services", "a2d130c5bd", [T, ":'week_appts'"]),
    ("Lịch: walk-in visits of the week", "d69e1c4f89", [T, ":'branch'", ":'monday'", ":'sunday'"]),
    ("Khách: search by name", "10c1809110", [T, "'%nguyen thi lan%'", "'nguyen thi lan%'", "20"]),
    ("Khách: search by phone digits", "7f47562028", [T, "'%1234%'", "20"]),
    ("Khách: recent", "606cf0e386", [T, "20"]),
    ("Khách: history page", "fbd2756aba", [T, ":'busy_customer'", TZ, "NULL", "NULL", "NULL", "NULL", "50", "NULL", "NULL", "NULL", "NULL"]),
    ("Khách: history visit lines", "cb7065c904", [T, ":'busy_visits'"]),
    ("Khách: summary", "9b121cbc0a", [T, ":'busy_customer'"]),
    ("Khách: package counts", "94aa10edc8", [T, ":'busy_customer'", ":'today'"]),
    ("Khách lâu chưa quay lại (inactive)", "ca27fc5c0d", [T, ":'today'", "30", "true", "NULL", "NULL", "50", "NULL", "NULL", "NULL", "'all'", "false", "NULL", "90"]),
    ("Gói: package overview list", "e0c7519b82", [T, ":'today'", "NULL", "NULL", "NULL", "NULL", "NULL", "50"]),
    ("Báo cáo: range 30 days (visits)", "3be7a5034a", [T, ":'d30'", "'day'", ":'today'"]),
    ("Báo cáo: range 366 days by month (visits)", "3be7a5034a", [T, ":'d365'", "'month'", ":'today'"]),
    ("Báo cáo: range 30 days (payments)", "347aef43f4", [T, ":'d30'", "'day'", ":'today'"]),
    ("Báo cáo: range 30 days (packages sold)", "da2e472123", [T, ":'d30'", "'day'", ":'today'"]),
    ("Báo cáo: top packages", "0efc8d0b51", [T, ":'d30'", ":'today'", "10"]),
    ("Báo cáo: daily technicians", "46e65d8514", [T, ":'today'"]),
    ("Sổ doanh thu: GET /reports/ledger month (rows)", "35e828234d", [T, ":'month_start'", ":'today'", "NULL", "NULL", "200"]),
    ("Sổ doanh thu: GET /reports/ledger month (day totals)", "9051c8873c", [T, ":'month_start'", ":'today'"]),
    ("Nhật ký ghi chú: GET /notes 30 days", "a6e8ba4b81",
     [T, TZ, ":'d30'", ":'today'"] + ["true"] * 9 + ["NULL", "NULL", "50", "NULL", "NULL", "NULL", "NULL", "NULL"]
     + ["true"] * 7 + ["NULL", "true"]),
    ("Nhân viên: attendance month", "98d9a48e64", [T, ":'month_start'", ":'month_end'", "NULL"]),
    ("Nhân viên: visit days of the month", "9dad675800", [T, ":'month_start'", ":'month_end'"]),
    ("Nhân viên: employment periods", "2b21464484", [T, "NULL"]),
    ("Nhân viên: daily reviews month", "7935a2fb45", [T, ":'month_start'", ":'month_end'", "NULL"]),
    ("Nhân viên: evaluations month", "f69d0de665", [T, ":'last_month'", "NULL"]),
    ("Lương: commission lines month", "64fbfdca95", [T, ":'last_month'", ":'last_month_end'"]),
    ("Lương: service lines month", "9d012671ae", [T, ":'last_month'", ":'last_month_end'"]),
    ("/me: upcoming appointments", "8b7ac22116", [T, ":'tech'", ":'today'", "20"]),
    ("/me: my visits today", "fd636b866c", [T, ":'tech'", ":'today'"]),
    ("/me/appointments week", "6260a40248", [T, ":'tech'", ":'monday'", ":'sunday'"]),
    ("/me/appointments week (walk-ins)", "f71935bf05", [T, ":'tech'", ":'monday'", ":'sunday'"]),
]


def statement(prefix):
    files = sorted(SQLX.glob(f"query-{prefix}*.json"))
    if not files:
        return None
    return json.loads(files[0].read_text(encoding="utf-8"))


def main():
    out = [CONTEXT]
    for n, (label, prefix, args) in enumerate(CASES):
        data = statement(prefix)
        out.append(f"\\echo '===== {n:02d} {label} [{prefix}]'")
        if data is None:
            out.append(f"\\echo 'NOTE: no sqlx statement with hash prefix {prefix} (query changed?)'")
            continue
        params = data["describe"]["parameters"]["Left"]
        if len(params) != len(args):
            out.append(f"\\echo 'NOTE: {prefix} takes {len(params)} parameters, case gives {len(args)}'")
            continue
        types = ", ".join(TYPES[p] for p in params)
        name = f"q{n:02d}"
        out.append(f"PREPARE {name}({types}) AS\n{data['query']};")
        out.append(f"EXPLAIN (ANALYZE, BUFFERS, SUMMARY) EXECUTE {name}({', '.join(args)});")
        out.append(f"DEALLOCATE {name};")
    sys.stdout.reconfigure(encoding="utf-8")
    print("\n".join(out))


if __name__ == "__main__":
    main()
