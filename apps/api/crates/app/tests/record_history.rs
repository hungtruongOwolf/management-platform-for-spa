//! "Ai ghi, ai sửa" on each record (`GET /record-history/...`, ADR-0022, spec record-history):
//! who may read it and in which order the rights are checked (AC-H1, AC-H4), what belongs to a
//! visit's and a package's history (D6, AC-H2), "Sửa lượt" read as one story (AC-H3), what counts
//! as an edit and the lists' `edited` agreeing with it (D9, D10, AC-H8), what each caller may see
//! (D7, D8, AC-H7) and where "Ghi bởi" comes from (D12).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{Datelike, FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use kernel::audit::{self, Action, AuditEntry};
use kernel::idempotency::hash_request;
use kernel::testing::app_pool;
use kernel::{Actor, TenantId, TenantTx, UserId};
use operations::dto::VisitCreate;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECHNICIAN_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";
const PLAN_NAME: &str = "Gội đầu 10 buổi + tặng 2";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    tenant: Uuid,
    owner: String,
    manager: String,
    technician: String,
    ha: String,
    lan: String,
    mai: String,
    shampoo: String,
    cheaper: String,
    plan: String,
    /// Hà's package (sold before every test, outside any visit) and its item.
    package: String,
    item: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn key() -> String {
    Uuid::now_v7().to_string()
}

fn spa_today() -> NaiveDate {
    let vn = FixedOffset::east_opt(7 * 3600).unwrap_or_else(|| panic!("offset"));
    Utc::now().with_timezone(&vn).date_naive()
}

async fn service(h: &Harness, name: &str) -> Setup<String> {
    let id: Uuid = sqlx::query_scalar(
        "SELECT s.id FROM app.service s JOIN app.service_category c ON c.id = s.category_id
         WHERE c.name = 'Gội đầu dưỡng sinh' AND s.name = $1",
    )
    .bind(name)
    .fetch_one(&h.owner)
    .await?;
    Ok(id.to_string())
}

async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Ngọc",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECHNICIAN_PHONE,
        "KTV Hoa",
        SystemRole::Technician,
    )
    .await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let technician = session_for(&h, TECHNICIAN_PHONE).await;
    let shampoo = service(&h, "Cơ Bản – Dầu thảo dược").await?;
    let cheaper = service(&h, "Cơ Bản – Dầu cặp").await?;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    let mai = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Mai", "Kỹ thuật viên").await?;
    let ha = post_customer(
        &h.app,
        Some(&manager),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female"})),
    )
    .await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": PLAN_NAME, "price": 1_500_000, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let sold = call_with_headers(
        &h.app,
        "POST",
        "/sales",
        Some(&manager),
        Some(json!({
            "customer_id": ha.body["id"], "closer_staff_id": lan.to_string(),
            "lines": [{"package_plan_id": plan.body["id"]}],
            "payments": [{"method": "cash", "amount": 1_500_000}],
        })),
        &[("idempotency-key", &key())],
    )
    .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    Ok(Spa {
        tenant,
        owner,
        manager,
        technician,
        ha: str_of(&ha.body["id"]),
        lan: lan.to_string(),
        mai: mai.to_string(),
        shampoo,
        cheaper,
        plan: str_of(&plan.body["id"]),
        package: str_of(&sold.body["customer_packages"][0]["id"]),
        item: str_of(&sold.body["customer_packages"][0]["items"][0]["id"]),
        h,
    })
}

impl Spa {
    fn one_off(&self, service: &str) -> Value {
        json!({"service_id": service, "source": "one_off", "staff_id": self.lan})
    }

    async fn post(&self, who: &str, path: &str, body: Value, if_match: Option<i64>) -> Reply {
        let k = key();
        let etag = if_match.map(|v| format!("\"{v}\""));
        let mut headers: Vec<(&str, &str)> = vec![("idempotency-key", &k)];
        if let Some(etag) = &etag {
            headers.push(("If-Match", etag));
        }
        call_with_headers(&self.h.app, "POST", path, Some(who), Some(body), &headers).await
    }

    /// A visit of Hà's, recorded by the manager.
    async fn record(&self, body: Value) -> Reply {
        let mut body = body;
        body["customer_id"] = json!(self.ha);
        let v = self.post(&self.manager, "/visits", body, None).await;
        assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
        v
    }

    async fn simple_visit(&self) -> Reply {
        self.record(json!({"lines": [self.one_off(&self.shampoo)],
                           "payments": [{"method": "cash", "amount": 49_000}]}))
            .await
    }

    async fn patch(&self, who: &str, visit: &str, version: i64, body: Value) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "PATCH",
            &format!("/visits/{visit}"),
            Some(who),
            Some(body),
            &[("If-Match", &etag)],
        )
        .await
    }

    async fn history(&self, who: &str, kind: &str, id: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            &format!("/record-history/{kind}/{id}"),
            Some(who),
            None,
        )
        .await
    }

    async fn owner_history(&self, kind: &str, id: &str) -> Value {
        let r = self.history(&self.owner, kind, id).await;
        assert_eq!(r.status, StatusCode::OK, "{kind} {id}: {}", r.body);
        r.body
    }

    async fn visit(&self, who: &str, id: &str) -> Value {
        call(
            &self.h.app,
            "GET",
            &format!("/visits/{id}"),
            Some(who),
            None,
        )
        .await
        .body
    }

    async fn exec(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }

    /// The operator's switch for the manager (ADR-0022 D2), and other grants, as stored rows.
    async fn grant_manager(&self, permission: &str, on: bool) -> Result<(), sqlx::Error> {
        let sql = if on {
            "INSERT INTO app.role_permission (tenant_id, role_id, permission)
             SELECT r.tenant_id, r.id, $2 FROM app.role r WHERE r.tenant_id = $1 AND r.key = 'manager'
             ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM app.role_permission rp USING app.role r
             WHERE r.tenant_id = rp.tenant_id AND r.id = rp.role_id
               AND r.tenant_id = $1 AND r.key = 'manager' AND rp.permission = $2"
        };
        sqlx::query(sql)
            .bind(self.tenant)
            .bind(permission)
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }
}

/// The actions of the lines, sorted: for what a history holds. Where the order is the point,
/// [`in_order`] (every request's lines carry the database clock of its writes, ADR-0023, so
/// requests made one after the other read in that order).
fn actions(history: &Value) -> Vec<String> {
    let mut all: Vec<String> = history["items"]
        .as_array()
        .map(|i| i.iter().map(|e| str_of(&e["action"])).collect())
        .unwrap_or_default();
    all.sort();
    all
}

/// The line of an action (the first one).
fn item<'a>(history: &'a Value, action: &str) -> &'a Value {
    history["items"]
        .as_array()
        .and_then(|i| i.iter().find(|e| e["action"] == action))
        .unwrap_or_else(|| panic!("no {action} in {history}"))
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
}

// ------------------------------------------------------------------------------- who reads it

/// AC-H1 / F17: `record.history.read` first (the owner by default, the manager once the operator
/// shows it, never the technician), then the id; another spa's record is not found; no-store.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn who_may_read_a_records_history(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s.simple_visit().await;
    let id = str_of(&v.body["id"]);

    let owner = s.history(&s.owner, "visit", &id).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(
        owner
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let summary = &owner.body["summary"];
    assert_eq!(summary["created"]["actor"]["name"], "Quản lý Ngọc");
    assert_eq!(summary["created"]["source"], "audit");
    assert!(summary["created"]["at"].is_string());
    assert_eq!(
        (&summary["edits"], &summary["state"]),
        (&json!(0), &json!("active"))
    );
    assert!(summary.get("last").is_none(), "{summary}");
    assert_eq!(owner.body["record"], json!({"type": "visit", "id": id}));
    assert_eq!(actions(&owner.body), ["visit.create"]);
    assert_eq!(owner.body["items"][0]["amount"], 49_000);

    for who in [&s.manager, &s.technician] {
        let refused = s.history(who, "visit", &id).await;
        assert_eq!(
            code(&refused),
            (StatusCode::FORBIDDEN, Some("auth.forbidden"))
        );
        assert!(refused.body.get("items").is_none());
        // No oracle: a malformed or unknown id is refused the same way.
        for junk in ["not-a-uuid", &Uuid::now_v7().to_string()] {
            let r = s.history(who, "visit", junk).await;
            assert_eq!(r.status, StatusCode::FORBIDDEN, "{junk}");
        }
    }
    assert_eq!(
        call(
            &s.h.app,
            "GET",
            &format!("/record-history/visit/{id}"),
            None,
            None
        )
        .await
        .status,
        StatusCode::UNAUTHORIZED
    );
    // With the rights: malformed → 400, unknown → 404.
    assert_eq!(
        s.history(&s.owner, "visit", "not-a-uuid").await.status,
        StatusCode::BAD_REQUEST
    );
    let unknown = Uuid::now_v7().to_string();
    for kind in ["visit", "sale", "refund", "customer-package", "customer"] {
        assert_eq!(
            s.history(&s.owner, kind, &unknown).await.status,
            StatusCode::NOT_FOUND,
            "{kind}"
        );
    }
    // A kind that has no route yet is the router's 404.
    assert_eq!(
        s.history(&s.owner, "staff", &id).await.status,
        StatusCode::NOT_FOUND
    );

    // The lists: no `edited` key without the right, a boolean with it.
    let day = spa_today();
    let list = call(
        &s.h.app,
        "GET",
        &format!("/visits?date={day}"),
        Some(&s.manager),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    assert!(list.body.to_string().contains(&id));
    assert!(
        !list.body.to_string().contains("\"edited\""),
        "{}",
        list.body
    );
    let mine = s.visit(&s.owner, &id).await;
    assert_eq!(mine["edited"], false);

    // The operator shows it to the manager: next request; hidden again: refused again.
    s.grant_manager("record.history.read", true).await?;
    assert_eq!(
        s.history(&s.manager, "visit", &id).await.status,
        StatusCode::OK
    );
    assert_eq!(s.visit(&s.manager, &id).await["edited"], false);
    s.grant_manager("record.history.read", false).await?;
    assert_eq!(
        s.history(&s.manager, "visit", &id).await.status,
        StatusCode::FORBIDDEN
    );

    // Another spa's owner: not found, whatever the kind.
    create_tenant(&s.h.owner, "khac", "Spa Khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "khac",
        OTHER_OWNER_PHONE,
        "Chủ Khác",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, OTHER_OWNER_PHONE).await;
    for (kind, record) in [
        ("visit", id.clone()),
        ("sale", str_of(&v.body["sale_id"])),
        ("customer-package", s.package.clone()),
        ("customer", s.ha.clone()),
    ] {
        assert_eq!(
            s.history(&other, kind, &record).await.status,
            StatusCode::NOT_FOUND,
            "{kind}"
        );
    }
    Ok(())
}

/// AC-H4 (B1): a sale's history is its visit's (one-off services) or its package's (a package
/// sale, by its `package` line); the anchor's read right is required before anything is read.
/// A refund is its package's.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_sale_is_read_as_its_visit_or_its_package(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(json!({"lines": [s.one_off(&s.shampoo)],
                       "package_sales": [{"package_plan_id": s.plan, "closer_staff_id": s.lan}],
                       "payments": [{"method": "cash", "amount": 1_549_000}]}))
        .await;
    let visit = str_of(&v.body["id"]);
    let one_off = str_of(&v.body["sale_id"]);
    let package_sale = str_of(&v.body["package_sales"][0]["sale_id"]);
    let package = str_of(&v.body["package_sales"][0]["customer_package_ids"][0]);
    assert!(!package.is_empty(), "{}", v.body);

    let owner = s.owner_history("sale", &one_off).await;
    assert_eq!(owner["record"], json!({"type": "visit", "id": visit}));
    let owner = s.owner_history("sale", &package_sale).await;
    assert_eq!(
        owner["record"],
        json!({"type": "customer_package", "id": package})
    );
    // The purchase line is the package sale itself, never the visit's line.
    assert_eq!(actions(&owner), ["sale.create"]);
    assert_eq!(owner["items"][0]["packages"], json!([PLAN_NAME]));
    assert!(!owner.to_string().contains("visit.create"), "{owner}");

    // sale.read + customer.read without visit.read: the one-off sale is refused, the package
    // sale is not.
    s.grant_manager("record.history.read", true).await?;
    s.grant_manager("visit.read", false).await?;
    let refused = s.history(&s.manager, "sale", &one_off).await;
    assert_eq!(
        code(&refused),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    assert_eq!(
        s.history(&s.manager, "visit", &visit).await.status,
        StatusCode::FORBIDDEN
    );
    let allowed = s.history(&s.manager, "sale", &package_sale).await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.body);
    assert_eq!(actions(&allowed.body), ["sale.create"]);
    // Still not found for an unknown sale (the route's rights are held).
    assert_eq!(
        s.history(&s.manager, "sale", &key()).await.status,
        StatusCode::NOT_FOUND
    );

    // A refund: the cancelled package's history.
    let cancelled = s
        .post(
            &s.owner,
            &format!("/customer-packages/{package}/cancel"),
            json!({"reason": "Khách chuyển nhà", "refund_amount": 300_000, "refund_method": "cash"}),
            None,
        )
        .await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
    let refund: Uuid =
        sqlx::query_scalar("SELECT id FROM app.refund WHERE customer_package_id = $1::uuid")
            .bind(&package)
            .fetch_one(&s.h.owner)
            .await?;
    let refund = refund.to_string();
    let by_refund = s.owner_history("refund", &refund).await;
    assert_eq!(
        by_refund["record"],
        json!({"type": "customer_package", "id": package})
    );
    assert_eq!(by_refund["summary"]["state"], "cancelled");
    assert!(actions(&by_refund).contains(&"customer_package.cancel".to_owned()));
    let manager = s.history(&s.manager, "refund", &refund).await;
    assert_eq!(manager.status, StatusCode::OK, "{}", manager.body);
    s.grant_manager("sale.read", false).await?;
    assert_eq!(
        s.history(&s.manager, "refund", &refund).await.status,
        StatusCode::FORBIDDEN
    );
    Ok(())
}

// ------------------------------------------------------------------------------- what is in it

/// AC-H2 (B2): a checkout is one line (both sales' money, the package named); the package's
/// later life (extended, cancelled with a refund) is in the package's history, never the visit's.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_visit_keeps_only_its_own_story(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let v = s
        .record(json!({"lines": [s.one_off(&s.shampoo)],
                       "package_sales": [{"package_plan_id": s.plan, "closer_staff_id": s.lan}],
                       "payments": [{"method": "cash", "amount": 1_549_000}]}))
        .await;
    let visit = str_of(&v.body["id"]);
    let package = str_of(&v.body["package_sales"][0]["customer_package_ids"][0]);
    let package_row = call(
        &s.h.app,
        "GET",
        &format!("/customer-packages/{package}"),
        Some(&s.owner),
        None,
    )
    .await;
    let version = package_row.body["version"].as_i64().unwrap_or(1);
    let extended = s
        .post(
            &s.owner,
            &format!("/customer-packages/{package}/extend"),
            json!({"expires_on": spa_today() + chrono::Duration::days(400), "reason": "Khách nằm viện"}),
            Some(version),
        )
        .await;
    assert_eq!(extended.status, StatusCode::OK, "{}", extended.body);
    let cancelled = s
        .post(
            &s.owner,
            &format!("/customer-packages/{package}/cancel"),
            json!({"reason": "Khách chuyển nhà", "refund_amount": 300_000, "refund_method": "cash"}),
            None,
        )
        .await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);

    let history = s.owner_history("visit", &visit).await;
    assert_eq!(actions(&history), ["visit.create"], "{history}");
    let line = &history["items"][0];
    assert_eq!(
        line["amount"], 1_549_000,
        "Thu = every sale of the checkout"
    );
    assert_eq!(line["packages"], json!([PLAN_NAME]));
    let stored: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE tenant_id = $1 AND action IN ('visit.create', 'sale.create')
           AND at = (SELECT at FROM app.audit_log WHERE entity_id = $2::uuid AND action = 'visit.create')",
    )
    .bind(s.tenant)
    .bind(&visit)
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(stored, 3, "the trail keeps every row");
    assert_eq!(history["summary"]["edits"], 0);
    assert_eq!(s.visit(&s.owner, &visit).await["edited"], false);

    let package_history = s.owner_history("customer-package", &package).await;
    assert_eq!(
        actions(&package_history),
        [
            "customer_package.cancel",
            "customer_package.extend",
            "sale.create"
        ]
    );
    assert_eq!(package_history["summary"]["created"]["source"], "audit");
    assert_eq!(
        package_history["summary"]["edits"], 0,
        "a package's life is not an edit"
    );
    assert_eq!(
        package_history["summary"]["last"]["actor"]["name"],
        "Chủ Spa"
    );
    Ok(())
}

/// AC-H3: A → B → C ("Sửa lượt" twice) is one story from any of the three; "Ghi bởi" is A's
/// creator; two edits; the lists mark every version.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_correction_chain_reads_as_one_story(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let a = s.simple_visit().await;
    let a_id = str_of(&a.body["id"]);
    let correct = |visit: String, lines: Value, amount: i64| {
        let s = &s;
        async move {
            let r = s
                .post(
                    &s.owner,
                    &format!("/visits/{visit}/correct"),
                    json!({"reason": "ghi nhầm", "customer_id": s.ha, "lines": lines,
                           "payments": [{"method": "cash", "amount": amount}]}),
                    Some(1),
                )
                .await;
            assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
            str_of(&r.body["visit"]["id"])
        }
    };
    let b_id = correct(a_id.clone(), json!([s.one_off(&s.cheaper)]), 39_000).await;
    let c_id = correct(
        b_id.clone(),
        json!([s.one_off(&s.cheaper), s.one_off(&s.shampoo)]),
        88_000,
    )
    .await;

    let stories: Vec<Value> = vec![
        s.owner_history("visit", &a_id).await,
        s.owner_history("visit", &b_id).await,
        s.owner_history("visit", &c_id).await,
    ];
    for story in &stories {
        assert_eq!(
            in_order(story),
            ["visit.create", "visit.correct", "visit.correct"],
            "{story}"
        );
        assert_eq!(story["items"], stories[0]["items"]);
        assert_eq!(story["summary"]["edits"], 2);
        assert_eq!(story["summary"]["created"]["actor"]["name"], "Quản lý Ngọc");
        assert_eq!(story["summary"]["last"]["actor"]["name"], "Chủ Spa");
    }
    let states: Vec<&Value> = stories.iter().map(|s| &s["summary"]["state"]).collect();
    assert_eq!(
        states,
        [&json!("replaced"), &json!("replaced"), &json!("active")]
    );
    for id in [&a_id, &b_id, &c_id] {
        assert_eq!(s.visit(&s.owner, id).await["edited"], true, "{id}");
    }

    // The revenue book: the replaced visits' sale rows (voided) and their void rows carry no
    // "Đã sửa"; the standing one does.
    let day = spa_today();
    let ledger = call(
        &s.h.app,
        "GET",
        &format!("/reports/ledger?from={day}&to={day}&limit=100"),
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(ledger.status, StatusCode::OK, "{}", ledger.body);
    let rows = ledger.body["rows"].as_array().cloned().unwrap_or_default();
    let c_rows: Vec<&Value> = rows
        .iter()
        .filter(|r| r["visit_id"] == json!(c_id))
        .collect();
    assert!(c_rows.iter().all(|r| r["edited"] == true), "{c_rows:?}");
    assert!(!c_rows.is_empty());
    for r in rows
        .iter()
        .filter(|r| r["entry"] == "void" || r["voided"] == true || r["kind"] == "package")
    {
        assert!(r.get("edited").is_none(), "{r}");
    }
    // Without the right, no key at all.
    s.grant_manager("report.revenue.read", true).await?;
    let manager_ledger = call(
        &s.h.app,
        "GET",
        &format!("/reports/ledger?from={day}&to={day}&limit=100"),
        Some(&s.manager),
        None,
    )
    .await;
    assert!(!manager_ledger.body.to_string().contains("\"edited\""));
    // The customer's history marks the visit events the same way.
    let timeline = call(
        &s.h.app,
        "GET",
        &format!("/customers/{}/history", s.ha),
        Some(&s.owner),
        None,
    )
    .await;
    let visits: Vec<&Value> = timeline.body["events"]
        .as_array()
        .map(|e| e.iter().filter(|x| x["visit"].is_object()).collect())
        .unwrap_or_default();
    assert!(visits.len() >= 3, "{}", timeline.body);
    assert!(visits.iter().all(|e| e["edited"] == true), "{visits:?}");
    let manager_timeline = call(
        &s.h.app,
        "GET",
        &format!("/customers/{}/history", s.ha),
        Some(&s.manager),
        None,
    )
    .await;
    assert!(!manager_timeline.body.to_string().contains("\"edited\""));
    Ok(())
}

/// D9 / AC-H1: an upsell, a re-sent technician and a session note are not edits; a technician
/// change is ("KTV: Lan → Mai", by name); a conversion to a package is one line and one edit. The
/// lists' `edited` is true exactly when the history counts an edit (D10).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn what_counts_as_an_edit_and_the_lists_agree(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // An upsell.
    let upsold = s.simple_visit().await;
    let upsold_id = str_of(&upsold.body["id"]);
    let added = s
        .post(
            &s.manager,
            &format!("/visits/{upsold_id}/additions"),
            json!({"lines": [s.one_off(&s.cheaper)], "payments": [{"method": "cash", "amount": 39_000}]}),
            None,
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    // The same technician sent back, then a real change.
    let resent = s.simple_visit().await;
    let resent_id = str_of(&resent.body["id"]);
    let line = str_of(&resent.body["lines"][0]["id"]);
    let same = s
        .patch(
            &s.manager,
            &resent_id,
            1,
            json!({"lines": [{"id": line, "staff_id": s.lan}]}),
        )
        .await;
    assert_eq!(same.status, StatusCode::OK, "{}", same.body);
    let moved = s.simple_visit().await;
    let moved_id = str_of(&moved.body["id"]);
    let moved_line = str_of(&moved.body["lines"][0]["id"]);
    let changed = s
        .patch(
            &s.manager,
            &moved_id,
            1,
            json!({"lines": [{"id": moved_line, "staff_id": s.mai}]}),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(
        changed.body["edited"],
        json!(null),
        "the manager has no right: no key"
    );
    // Only the session note.
    let noted = s.simple_visit().await;
    let noted_id = str_of(&noted.body["id"]);
    let note = s
        .patch(&s.manager, &noted_id, 1, json!({"note": "Đau vai gáy"}))
        .await;
    assert_eq!(note.status, StatusCode::OK, "{}", note.body);
    // A conversion to a package.
    let converted = s.simple_visit().await;
    let converted_id = str_of(&converted.body["id"]);
    let conversion = s
        .post(
            &s.manager,
            &format!("/visits/{converted_id}/convert-to-package"),
            json!({"package_sale": {"package_plan_id": s.plan, "closer_staff_id": s.lan},
                   "payments": [{"method": "cash", "amount": 1_451_000}]}),
            Some(1),
        )
        .await;
    assert_eq!(
        conversion.status,
        StatusCode::CREATED,
        "{}",
        conversion.body
    );
    let replacement = str_of(&conversion.body["visit"]["id"]);

    let upsell = s.owner_history("visit", &upsold_id).await;
    assert_eq!(actions(&upsell), ["visit.addition", "visit.create"]);
    let resend = s.owner_history("visit", &resent_id).await;
    assert_eq!(actions(&resend), ["visit.create", "visit.update"]);
    let technician = s.owner_history("visit", &moved_id).await;
    assert_eq!(
        item(&technician, "visit.update")["changes"],
        json!([{"field": "staff", "format": "text", "old": "KTV Lan", "new": "KTV Mai"}])
    );
    let session_note = s.owner_history("visit", &noted_id).await;
    assert_eq!(
        item(&session_note, "visit.update")["changes"],
        json!([{"field": "session_note", "format": "flag", "new": "true"}])
    );
    assert!(!session_note.to_string().contains("Đau vai gáy"));
    let conversion_story = s.owner_history("visit", &converted_id).await;
    assert_eq!(
        actions(&conversion_story),
        ["visit.convert_to_package", "visit.create"]
    );
    assert_eq!(
        item(&conversion_story, "visit.convert_to_package")["packages"],
        json!([PLAN_NAME])
    );

    let expected = [
        (&upsold_id, 0),
        (&resent_id, 0),
        (&moved_id, 1),
        (&noted_id, 0),
        (&converted_id, 1),
        (&replacement, 1),
    ];
    for (id, edits) in expected {
        let story = s.owner_history("visit", id).await;
        assert_eq!(story["summary"]["edits"], edits, "{id}: {story}");
        let edited = s.visit(&s.owner, id).await["edited"].clone();
        assert_eq!(
            edited,
            json!(edits >= 1),
            "edited agrees with edits for {id}"
        );
    }
    // The day list says the same.
    let day = spa_today();
    let list = call(
        &s.h.app,
        "GET",
        &format!("/visits?date={day}"),
        Some(&s.owner),
        None,
    )
    .await;
    for row in list.body.as_array().cloned().unwrap_or_default() {
        let story = s.owner_history("visit", &str_of(&row["id"])).await;
        assert_eq!(
            row["edited"],
            json!(story["summary"]["edits"].as_i64() >= Some(1)),
            "{row}"
        );
    }

    // Without the health-notes right the note-only edit is not there at all; a profile note is.
    s.grant_manager("record.history.read", true).await?;
    s.grant_manager("customer.health_notes.read", false).await?;
    let blind = s.history(&s.manager, "visit", &noted_id).await;
    assert_eq!(actions(&blind.body), ["visit.create"], "{}", blind.body);
    // Without sale.read, no money.
    s.grant_manager("sale.read", false).await?;
    let no_money = s.history(&s.manager, "visit", &upsold_id).await;
    assert_eq!(no_money.status, StatusCode::OK, "{}", no_money.body);
    assert!(
        !no_money.body.to_string().contains("\"amount\""),
        "{}",
        no_money.body
    );
    assert!(!no_money.body.to_string().contains("\"discount\""));
    Ok(())
}

/// D7: an expired package used with an override is in the visit's and the package's history,
/// found through the package the visit used.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn an_expired_package_override_shows_on_both(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    s.exec(&format!(
        "UPDATE app.customer_package SET starts_on = current_date - 60, expires_on = current_date - 1
         WHERE id = '{}'",
        s.package
    ))
    .await?;
    let v = s
        .post(
            &s.owner,
            "/visits",
            json!({"customer_id": s.ha, "lines": [{
                "service_id": s.shampoo, "source": "package", "customer_package_item_id": s.item,
                "staff_id": s.lan, "expiry_override_reason": "Khách ốm nằm viện 2 tháng"}]}),
            None,
        )
        .await;
    assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
    let visit = str_of(&v.body["id"]);
    let story = s.owner_history("visit", &visit).await;
    let seen = actions(&story);
    assert_eq!(
        seen,
        ["customer_package.use_expired_override", "visit.create"],
        "{story}"
    );
    assert_eq!(story["summary"]["edits"], 0);
    let package = s.owner_history("customer-package", &s.package).await;
    assert!(actions(&package).contains(&"customer_package.use_expired_override".to_owned()));
    // Another visit's override is not this one's.
    let other = s.simple_visit().await;
    let other_story = s.owner_history("visit", &str_of(&other.body["id"])).await;
    assert_eq!(actions(&other_story), ["visit.create"]);
    Ok(())
}

/// AC-H7 / D8: the owner sees the old and new phone; a manager shown the history sees that the
/// phone changed, with no digit of either number; names show for both. An anonymised customer has
/// neither. A health-note change is only for the health-notes right; the profile note is not one.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_customer_profiles_history_keeps_phones_to_the_owner(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let renamed = call_with_headers(
        &s.h.app,
        "PUT",
        &format!("/customers/{}", s.ha),
        Some(&s.manager),
        Some(
            json!({"full_name": "Nguyễn Thị Thu Hà", "phone": "0912333444", "gender": "female",
                    "note": "Thích phòng yên tĩnh"}),
        ),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);

    let owner = s.owner_history("customer", &s.ha).await;
    assert_eq!(owner["record"]["type"], "customer");
    assert_eq!(actions(&owner), ["customer.create", "customer.update"]);
    assert_eq!(owner["summary"]["edits"], 1);
    assert_eq!(owner["summary"]["created"]["actor"]["name"], "Quản lý Ngọc");
    let changes = item(&owner, "customer.update")["changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let phone = changes
        .iter()
        .find(|c| c["field"] == "phone")
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (&phone["old"], &phone["new"]),
        (&json!("+84905111222"), &json!("+84912333444"))
    );

    s.grant_manager("record.history.read", true).await?;
    let manager = s.history(&s.manager, "customer", &s.ha).await;
    assert_eq!(manager.status, StatusCode::OK, "{}", manager.body);
    let text = manager.body.to_string();
    for digits in ["905111222", "912333444", "0905", "0912"] {
        assert!(!text.contains(digits), "{digits} in {text}");
    }
    let changes = item(&manager.body, "customer.update")["changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let phone = changes
        .iter()
        .find(|c| c["field"] == "phone")
        .cloned()
        .unwrap_or_default();
    assert_eq!(phone, json!({"field": "phone", "format": "phone"}));
    let name = changes
        .iter()
        .find(|c| c["field"] == "full_name")
        .cloned()
        .unwrap_or_default();
    assert_eq!(name["new"], "Nguyễn Thị Thu Hà");
    // A health-note change: only with the right; the profile note change stays without it.
    let health = call(
        &s.h.app,
        "PUT",
        &format!("/customers/{}/health-note", s.ha),
        Some(&s.owner),
        Some(json!({"health_note": "Huyết áp cao"})),
    )
    .await;
    if health.status.is_success() {
        let owner = s.owner_history("customer", &s.ha).await;
        assert!(actions(&owner).contains(&"customer.health_note_set".to_owned()));
        assert!(!owner.to_string().contains("Huyết áp cao"));
        s.grant_manager("customer.health_notes.read", false).await?;
        let blind = s.history(&s.manager, "customer", &s.ha).await;
        assert_eq!(actions(&blind.body), ["customer.create", "customer.update"]);
    }
    // Anonymised: no name, no values.
    s.exec(&format!(
        "UPDATE app.customer SET anonymized_at = now() WHERE id = '{}'",
        s.ha
    ))
    .await?;
    let gone = s.owner_history("customer", &s.ha).await;
    let text = gone.to_string();
    assert!(
        !text.contains("Nguyễn Thị") && !text.contains("+84"),
        "{text}"
    );
    Ok(())
}

// ------------------------------------------------------------------------------- "Ghi bởi"

/// D12 + edge cases 6, 8, 9, 16: the creating action is "Ghi bởi"; without one, the record's own
/// columns (`source = record`); a paper-book package reads "Nhập từ sổ giấy" with its book's day
/// even after it is extended; a customer created by the import keeps the importer; times are the
/// trail's own (a back-dated seed keeps its time); a person who left reads `left`.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn ghi_boi_comes_from_the_creating_action(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // A back-dated trail row keeps its own time.
    let v = s.simple_visit().await;
    let visit = str_of(&v.body["id"]);
    let earlier = "2026-01-05T03:10:00Z";
    s.exec(&format!(
        "UPDATE app.audit_log SET at = '{earlier}' WHERE entity_id = '{visit}' AND action = 'visit.create'"
    ))
    .await?;
    let story = s.owner_history("visit", &visit).await;
    assert_eq!(story["summary"]["created"]["at"], earlier);
    assert_eq!(story["summary"]["created"]["day"], "2026-01-05");
    // Moved apart from its sale's row, the sale is its own line (it no longer folds).
    assert_eq!(actions(&story), ["sale.create", "visit.create"], "{story}");
    // No creating row: the visit's own columns (its sale's row stays a line of the visit).
    s.exec(&format!(
        "DELETE FROM app.audit_log WHERE entity_id = '{visit}'"
    ))
    .await?;
    let story = s.owner_history("visit", &visit).await;
    assert_eq!(story["summary"]["created"]["source"], "record");
    assert_eq!(story["summary"]["created"]["actor"]["name"], "Quản lý Ngọc");
    assert_eq!(actions(&story), ["sale.create"]);
    // The manager leaves: their lines read "(đã nghỉ)".
    s.exec(&format!(
        "UPDATE app.membership SET status = 'disabled' WHERE tenant_id = '{}'
           AND user_id = (SELECT id FROM app.user_account WHERE phone_e164 = '+84912000111')",
        s.tenant
    ))
    .await?;
    let story = s.owner_history("visit", &visit).await;
    assert_eq!(story["summary"]["created"]["actor"]["left"], true);
    let package = s.owner_history("customer-package", &s.package).await;
    assert_eq!(package["items"][0]["actor"]["left"], true, "{package}");

    // A paper-book import that creates the customer, then the package is extended.
    let imported = s
        .post(
            &s.owner,
            "/opening-balances",
            json!({
                "customer": {"full_name": "Trần Văn Bình", "phone": "0914 555 666", "gender": "male"},
                "packages": [{
                    "name": "Gội đầu 10 buổi (sổ cũ)", "sold_price": 1_500_000,
                    "starts_on": "2026-06-01", "expires_on": "2026-12-01",
                    "items": [{"service_id": s.shampoo, "sessions_purchased": 10, "sessions_remaining": 4}],
                }],
            }),
            None,
        )
        .await;
    assert_eq!(imported.status, StatusCode::CREATED, "{}", imported.body);
    let customer = str_of(&imported.body["customer_id"]);
    let paper = str_of(&imported.body["customer_packages"][0]["id"]);
    let version = imported.body["customer_packages"][0]["version"]
        .as_i64()
        .unwrap_or(1);
    let extended = s
        .post(
            &s.owner,
            &format!("/customer-packages/{paper}/extend"),
            json!({"expires_on": spa_today() + chrono::Duration::days(90), "reason": "Gia hạn"}),
            Some(version),
        )
        .await;
    assert_eq!(extended.status, StatusCode::OK, "{}", extended.body);
    let paper_story = s.owner_history("customer-package", &paper).await;
    assert_eq!(
        paper_story["summary"]["created"],
        json!({"actor": {"kind": "user", "user_id": paper_story["summary"]["created"]["actor"]["user_id"],
                         "name": "Chủ Spa"},
               "day": "2026-06-01", "source": "paper_book"}),
        "{paper_story}"
    );
    assert_eq!(
        actions(&paper_story),
        ["customer_package.extend", "opening_balance.import"],
        "one line says it was imported"
    );
    let updated = call_with_headers(
        &s.h.app,
        "PUT",
        &format!("/customers/{customer}"),
        Some(&s.owner),
        Some(json!({"full_name": "Trần Văn Bình", "phone": "0914555777", "gender": "male"})),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
    let customer_story = s.owner_history("customer", &customer).await;
    assert_eq!(customer_story["summary"]["created"]["source"], "paper_book");
    assert_eq!(
        customer_story["summary"]["created"]["actor"]["name"],
        "Chủ Spa"
    );
    assert_eq!(customer_story["summary"]["edits"], 1);
    Ok(())
}

// ------------------------------------------------------------------- wave 2: the books, bookings

const SECOND_MANAGER_PHONE: &str = "0912000444";

/// `+7` business day `days` from today at `hm` (spa time).
fn at(days: i64, hm: &str) -> String {
    format!(
        "{}T{hm}:00+07:00",
        spa_today() + chrono::Duration::days(days)
    )
}

impl Spa {
    async fn put(&self, who: &str, path: &str, body: Value, idempotent: bool) -> Reply {
        let k = key();
        let headers: Vec<(&str, &str)> = if idempotent {
            vec![("idempotency-key", &k)]
        } else {
            Vec::new()
        };
        call_with_headers(&self.h.app, "PUT", path, Some(who), Some(body), &headers).await
    }

    /// A staff profile linked to the login with this phone (a manager's own, the owner's).
    async fn link(&self, phone: &str, name: &str) -> Setup<String> {
        let user: Uuid =
            sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
                .bind(format!("+84{}", &phone[1..]))
                .fetch_one(&self.h.owner)
                .await?;
        let r = call(
            &self.h.app,
            "POST",
            "/staff",
            Some(&self.owner),
            Some(json!({"full_name": name, "position": "Quản lý", "user_id": user})),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
        Ok(str_of(&r.body["id"]))
    }

    async fn staff_day(&self, who: &str, staff: &str, day: &str, book: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            &format!("/record-history/staff-day/{staff}/{day}?book={book}"),
            Some(who),
            None,
        )
        .await
    }

    async fn calendar(&self, who: &str, day: &str, marks: bool) -> Value {
        let q = if marks { "&marks=true" } else { "" };
        let r = call(
            &self.h.app,
            "GET",
            &format!("/appointments?from={day}&to={day}{q}"),
            Some(who),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        r.body
    }

    /// Switches a feature off for the spa, as an operator's switch leaves it.
    async fn switch_off(&self, feature: &str) -> Setup<()> {
        let operator = common::add_operator(&self.h).await?;
        sqlx::query(
            "INSERT INTO app.tenant_feature (tenant_id, feature, enabled, updated_by)
             VALUES ($1, $2, false, $3)
             ON CONFLICT (tenant_id, feature) DO UPDATE SET enabled = false",
        )
        .bind(self.tenant)
        .bind(feature)
        .bind(operator)
        .execute(&self.h.owner)
        .await?;
        Ok(())
    }
}

/// The lines of a history, each as (action, the fields it changed).
fn lines(history: &Value) -> Vec<(String, Vec<String>)> {
    let mut all: Vec<(String, Vec<String>)> = history["items"]
        .as_array()
        .map(|i| {
            i.iter()
                .map(|e| {
                    let fields = e["changes"]
                        .as_array()
                        .map(|c| c.iter().map(|c| str_of(&c["field"])).collect())
                        .unwrap_or_default();
                    (str_of(&e["action"]), fields)
                })
                .collect()
        })
        .unwrap_or_default();
    all.sort();
    all
}

/// AC-H1 / AC-H8 for a booking (spec §4.1, D9): who made it, what changed, a move or another
/// technician counts as an edit (a note, the services, the cancellation do not), and the list
/// view's `edited` agrees with "đã sửa N lần"; only the list asks (`marks=true`), only the history
/// right gets it, the answers of the actions carry it; the appointments feature off answers
/// `403 feature.disabled` from the route guard.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_bookings_history_and_its_list_mark(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let day = (spa_today() + chrono::Duration::days(1)).to_string();
    let booked = s
        .post(
            &s.manager,
            "/appointments",
            json!({"customer_id": s.ha, "service_ids": [s.shampoo], "starts_at": at(1, "10:00")}),
            None,
        )
        .await;
    assert_eq!(booked.status, StatusCode::CREATED, "{}", booked.body);
    assert!(booked.body.get("edited").is_none(), "no right, no key");
    let id = str_of(&booked.body["id"]);
    let story = s.owner_history("appointment", &id).await;
    assert_eq!(story["record"], json!({"type": "appointment", "id": id}));
    assert_eq!(actions(&story), ["appointment.create"]);
    assert_eq!(
        (
            &story["summary"]["created"]["actor"]["name"],
            &story["summary"]["edits"]
        ),
        (&json!("Quản lý Ngọc"), &json!(0))
    );
    for who in [&s.manager, &s.technician] {
        let refused = s.history(who, "appointment", &id).await;
        assert_eq!(
            code(&refused),
            (StatusCode::FORBIDDEN, Some("auth.forbidden"))
        );
    }
    // The mark: only the list asks, only the history right gets it.
    let mark = |cal: &Value| cal["appointments"][0].get("edited").cloned();
    assert_eq!(
        mark(&s.calendar(&s.owner, &day, true).await),
        Some(json!(false))
    );
    assert_eq!(mark(&s.calendar(&s.owner, &day, false).await), None);
    assert_eq!(mark(&s.calendar(&s.manager, &day, true).await), None);

    let patch = |who: &str, version: i64, body: Value| {
        let etag = format!("\"{version}\"");
        let (app, path) = (s.h.app.clone(), format!("/appointments/{id}"));
        let who = who.to_owned();
        async move {
            call_with_headers(
                &app,
                "PATCH",
                &path,
                Some(&who),
                Some(body),
                &[("If-Match", &etag)],
            )
            .await
        }
    };
    // The note only: said, never its text, no edit.
    let noted = patch(&s.manager, 1, json!({"note": "Khách hỏi chỗ đỗ xe"})).await;
    assert_eq!(noted.status, StatusCode::OK, "{}", noted.body);
    let story = s.owner_history("appointment", &id).await;
    assert_eq!(
        lines(&story),
        [
            ("appointment.create".to_owned(), vec![]),
            (
                "appointment.update".to_owned(),
                vec!["booking_note".to_owned()]
            )
        ]
    );
    assert!(!story.to_string().contains("đỗ xe"), "{story}");
    assert_eq!(story["summary"]["edits"], 0);
    assert_eq!(
        mark(&s.calendar(&s.owner, &day, true).await),
        Some(json!(false))
    );
    // Moved: an edit, and the owner's answer says so at once.
    let moved = patch(&s.owner, 2, json!({"starts_at": at(1, "11:00")})).await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["edited"], true);
    // Another technician: a second one.
    let staffed = patch(&s.owner, 3, json!({"staff_id": s.lan})).await;
    assert_eq!(staffed.status, StatusCode::OK, "{}", staffed.body);
    let story = s.owner_history("appointment", &id).await;
    assert_eq!(story["summary"]["edits"], 2, "{story}");
    let staff = story["items"]
        .as_array()
        .and_then(|i| {
            i.iter()
                .flat_map(|e| e["changes"].as_array().cloned().unwrap_or_default())
                .find(|c| c["field"] == "staff")
        })
        .unwrap_or_default();
    assert_eq!(
        (staff.get("old"), &staff["new"]),
        (None, &json!("KTV Lan")),
        "{staff}"
    );
    assert_eq!(
        mark(&s.calendar(&s.owner, &day, true).await),
        Some(json!(true))
    );
    let one = call(
        &s.h.app,
        "GET",
        &format!("/appointments/{id}"),
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(one.body["edited"], true);
    // Cancelled: its life, no edit; the history says it is cancelled, with the reason.
    let cancelled = s
        .post(
            &s.manager,
            &format!("/appointments/{id}/cancel"),
            json!({"reason": "Khách bận đột xuất"}),
            None,
        )
        .await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
    let story = s.owner_history("appointment", &id).await;
    assert_eq!(
        (&story["summary"]["edits"], &story["summary"]["state"]),
        (&json!(2), &json!("cancelled"))
    );
    assert_eq!(
        item(&story, "appointment.cancel")["reason"],
        "Khách bận đột xuất"
    );
    // With the rights: malformed → 400, unknown → 404.
    assert_eq!(
        s.history(&s.owner, "appointment", "not-a-uuid")
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        s.history(&s.owner, "appointment", &Uuid::now_v7().to_string())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // The appointments feature off: the route guard answers, before any handler.
    s.switch_off("appointments").await?;
    let off = s.history(&s.owner, "appointment", &id).await;
    assert_eq!(
        code(&off),
        (StatusCode::FORBIDDEN, Some("feature.disabled"))
    );
    Ok(())
}

/// Spec edge case 13, threat model N4 (IDOR on a cell): a staff-book cell is a person and a day.
/// One save of the book wrote several cells at one instant, yet each cell's history holds only
/// its own day; every write after the first is an edit; a note's text never shows ("có sửa"); a
/// cleared cell reads "xóa chấm công". The caller's books decide: their own cell `404`, another
/// manager's and the owner's `200`, a month the person did not work `404`.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_staff_day_cell_keeps_its_own_day_and_the_callers_books(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ngoc = s.link(MANAGER_PHONE, "Quản lý Ngọc").await?;
    add_member(
        &s.h.owner,
        "vpyd",
        SECOND_MANAGER_PHONE,
        "Quản lý Thu",
        SystemRole::Manager,
    )
    .await?;
    let thu = s.link(SECOND_MANAGER_PHONE, "Quản lý Thu").await?;
    let boss = s.link(OWNER_PHONE, "Chủ Spa").await?;
    let d1 = spa_today() - chrono::Duration::days(1);
    let d2 = spa_today() - chrono::Duration::days(2);
    let (d1s, d2s) = (d1.to_string(), d2.to_string());
    let present = |staff: &str, day: &str, check_in: &str, note: Option<&str>| {
        json!({"staff_id": staff, "date": day, "status": "present", "check_in": check_in,
               "note": note})
    };
    // One save of the book: five cells, two days.
    let saved = s
        .put(
            &s.owner,
            "/attendance",
            json!({"entries": [
                present(&s.lan, &d1s, "09:05", Some("Đi muộn vì kẹt xe")),
                present(&s.lan, &d2s, "09:00", None),
                present(&thu, &d1s, "08:55", None),
                present(&boss, &d1s, "08:50", None),
                present(&ngoc, &d1s, "08:58", None),
            ]}),
            false,
        )
        .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    // Lan's first day again: a later check-in, then only the note's text; her second day cleared.
    for entry in [
        present(&s.lan, &d1s, "09:20", Some("Đi muộn vì kẹt xe")),
        present(&s.lan, &d1s, "09:20", Some("Kẹt xe ở cầu Rồng")),
        json!({"staff_id": s.lan, "date": d2s, "status": null}),
    ] {
        let r = s
            .put(
                &s.manager,
                "/attendance",
                json!({"entries": [entry]}),
                false,
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
    let praised = s
        .put(
            &s.owner,
            "/daily-reviews",
            json!({"entries": [{"staff_id": s.lan, "date": d1s, "compliments": 1, "complaints": 0,
                                "note": "Khách khen tay nghề"}]}),
            false,
        )
        .await;
    assert_eq!(praised.status, StatusCode::OK, "{}", praised.body);

    let cell = s.staff_day(&s.owner, &s.lan, &d1s, "attendance").await;
    assert_eq!(cell.status, StatusCode::OK, "{}", cell.body);
    assert_eq!(
        cell.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let h = &cell.body;
    assert_eq!(h["record"], json!({"type": "attendance"}));
    let items = h["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 3, "{h}");
    assert!(
        items
            .iter()
            .all(|e| e["action"] == "attendance.record" && e["subject"]["on"] == json!(d1s)),
        "only this day: {h}"
    );
    // The first write says what it set (the note as "Có", never its text); the later ones what
    // they changed: the check-in, then only the note's text ("Ghi chú (đã đổi)", K25).
    let strings = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        lines(h),
        [
            ("attendance.record".to_owned(), strings(&["check_in"])),
            ("attendance.record".to_owned(), strings(&["has_note"])),
            (
                "attendance.record".to_owned(),
                strings(&["status", "check_in", "has_note"])
            ),
        ]
    );
    let note_edit = items
        .iter()
        .filter_map(|e| e["changes"].as_array().cloned())
        .find(|c| c.len() == 1 && c[0]["field"] == "has_note")
        .and_then(|c| c.first().cloned())
        .unwrap_or_default();
    assert_eq!(note_edit["field"], "has_note", "{h}");
    assert!(
        note_edit.get("old").is_none() && note_edit.get("new").is_none(),
        "{note_edit}"
    );
    for text in ["kẹt xe", "Kẹt xe", "cầu Rồng", "tay nghề"] {
        assert!(!h.to_string().contains(text), "{text}: {h}");
    }
    assert_eq!(
        (
            &h["summary"]["created"]["actor"]["name"],
            &h["summary"]["edits"]
        ),
        (&json!("Chủ Spa"), &json!(2))
    );
    assert_eq!(h["summary"]["last"]["actor"]["name"], "Quản lý Ngọc");
    // The second day: its own two rows, the clear says so.
    let other = s.staff_day(&s.owner, &s.lan, &d2s, "attendance").await;
    let items = other.body["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 2, "{}", other.body);
    assert!(items.iter().all(|e| e["subject"]["on"] == json!(d2s)));
    assert!(items.iter().any(|e| e["detail"] == "cleared"));
    assert_eq!(other.body["summary"]["edits"], 1);
    // The other book of the same day: its own row only.
    let review = s.staff_day(&s.owner, &s.lan, &d1s, "daily_review").await;
    assert_eq!(review.status, StatusCode::OK, "{}", review.body);
    assert_eq!(review.body["record"], json!({"type": "daily_review"}));
    assert_eq!(actions(&review.body), ["evaluation.daily_review"]);
    assert_eq!(review.body["summary"]["edits"], 0);
    assert!(!review.body.to_string().contains("tay nghề"));
    // A day nobody wrote: empty, nobody named.
    let empty = s
        .staff_day(&s.owner, &s.mai, &spa_today().to_string(), "attendance")
        .await;
    assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
    assert_eq!(empty.body["items"], json!([]));
    assert!(empty.body["summary"].get("created").is_none());

    // Who may read it: the switch first, then the book's right, then the caller's books.
    for who in [&s.manager, &s.technician] {
        let refused = s.staff_day(who, &s.lan, &d1s, "attendance").await;
        assert_eq!(
            code(&refused),
            (StatusCode::FORBIDDEN, Some("auth.forbidden"))
        );
        // No oracle: junk is refused the same way.
        let junk = s.staff_day(who, "x", "y", "z").await;
        assert_eq!(junk.status, StatusCode::FORBIDDEN);
    }
    s.grant_manager("record.history.read", true).await?;
    assert_eq!(
        s.staff_day(&s.manager, &ngoc, &d1s, "attendance")
            .await
            .status,
        StatusCode::NOT_FOUND,
        "their own cell is not in their books"
    );
    for (who, staff) in [
        ("another manager", &thu),
        ("the owner", &boss),
        ("a technician", &s.lan),
    ] {
        let r = s.staff_day(&s.manager, staff, &d1s, "attendance").await;
        assert_eq!(r.status, StatusCode::OK, "{who}: {}", r.body);
    }
    // Without the book's right: 403 for that book only.
    s.grant_manager("evaluation.record", false).await?;
    assert_eq!(
        code(&s.staff_day(&s.manager, &s.lan, &d1s, "daily_review").await),
        (StatusCode::FORBIDDEN, Some("auth.forbidden"))
    );
    assert_eq!(
        s.staff_day(&s.manager, &s.lan, &d1s, "attendance")
            .await
            .status,
        StatusCode::OK
    );
    // A month the person did not work: not in the books.
    let first = spa_today().with_day(1).unwrap_or_else(spa_today);
    let newcomer = call(
        &s.h.app,
        "POST",
        "/staff",
        Some(&s.owner),
        Some(json!({"full_name": "KTV Hằng", "position": "Kỹ thuật viên", "joined_on": first})),
    )
    .await;
    assert_eq!(newcomer.status, StatusCode::CREATED, "{}", newcomer.body);
    let hang = str_of(&newcomer.body["id"]);
    let before = (first - chrono::Duration::days(1)).to_string();
    assert_eq!(
        s.staff_day(&s.owner, &hang, &before, "attendance")
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        s.staff_day(&s.owner, &hang, &first.to_string(), "attendance")
            .await
            .status,
        StatusCode::OK
    );
    // With the rights: a bad book, a bad date, a bad id → 400; someone else's → 404.
    for (staff, day, book) in [
        (s.lan.as_str(), d1s.as_str(), "payroll"),
        (s.lan.as_str(), "2026-13-40", "attendance"),
        ("x", d1s.as_str(), "attendance"),
    ] {
        let r = s.staff_day(&s.owner, staff, day, book).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{staff} {day} {book}");
    }
    let no_book = call(
        &s.h.app,
        "GET",
        &format!("/record-history/staff-day/{}/{d1s}", s.lan),
        Some(&s.owner),
        None,
    )
    .await;
    assert_eq!(no_book.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        s.staff_day(&s.owner, &Uuid::now_v7().to_string(), &d1s, "attendance")
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    Ok(())
}

/// Spec D11 / §4.1 (S13): a bonus / penalty edited is one chain, read the same from any of its
/// entries, "đã sửa 1 lần" per edit (the void and the new entry are one line of what changed);
/// moved to another person it is still one chain, and every person of it must be in the caller's
/// books; a removal is no edit.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_bonus_and_its_edits_read_as_one_chain(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let ngoc = s.link(MANAGER_PHONE, "Quản lý Ngọc").await?;
    let day = spa_today().to_string();
    let bonus = |staff: &str, amount: i64| {
        json!({"staff_id": staff, "kind": "bonus", "amount": amount,
               "reason": "Khách khen đích danh", "occurred_on": day})
    };
    let record = |who: &str, body: Value| {
        let (s, who) = (&s, who.to_owned());
        async move {
            let r = s.post(&who, "/adjustments", body, None).await;
            assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
            str_of(&r.body["id"])
        }
    };
    let edit = |who: &str, id: &str, body: Value| {
        let (s, who, path) = (&s, who.to_owned(), format!("/adjustments/{id}"));
        async move {
            let r = s.put(&who, &path, body, true).await;
            assert_eq!(r.status, StatusCode::OK, "{}", r.body);
            str_of(&r.body["id"])
        }
    };
    let first = record(&s.owner, bonus(&s.lan, 200_000)).await;
    let second = edit(&s.manager, &first, bonus(&s.lan, 300_000)).await;
    let story = s.owner_history("adjustment", &second).await;
    assert_eq!(story["record"], json!({"type": "adjustment", "id": second}));
    assert_eq!(story["items"].as_array().map(Vec::len), Some(2), "{story}");
    assert_eq!(
        (
            &story["summary"]["edits"],
            &story["summary"]["state"],
            &story["summary"]["created"]["actor"]["name"]
        ),
        (&json!(1), &json!("active"), &json!("Chủ Spa"))
    );
    let edited = story["items"]
        .as_array()
        .and_then(|i| {
            i.iter()
                .find(|e| e["folded"] == json!(["staff.adjustment_void"]))
        })
        .cloned()
        .unwrap_or_default();
    assert_eq!(edited["action"], "staff.adjustment");
    assert_eq!(edited["actor"]["name"], "Quản lý Ngọc");
    assert_eq!(
        edited["changes"],
        json!([{"field": "amount", "format": "money", "old": "200000", "new": "300000"}])
    );
    assert!(edited.get("amount").is_none(), "{edited}");
    // From the replaced entry: the same story.
    let from_first = s.owner_history("adjustment", &first).await;
    assert_eq!(from_first["items"], story["items"]);
    assert_eq!(from_first["summary"]["state"], "replaced");
    // Moved to Mai: still one chain, two edits; the person changes by name.
    let third = edit(&s.owner, &second, bonus(&s.mai, 300_000)).await;
    let story = s.owner_history("adjustment", &third).await;
    assert_eq!(story["items"].as_array().map(Vec::len), Some(3), "{story}");
    assert_eq!(story["summary"]["edits"], 2);
    assert!(
        story["items"].as_array().is_some_and(|i| {
            i.iter().any(|e| e["changes"]
            == json!([{"field": "person", "format": "text", "old": "KTV Lan", "new": "KTV Mai"}]))
        }),
        "{story}"
    );
    // The scope covers every person of the chain (S13): through the manager's own profile → 404.
    let own = record(&s.owner, bonus(&ngoc, 100_000)).await;
    let moved = edit(&s.owner, &own, bonus(&s.lan, 100_000)).await;
    assert_eq!(
        code(&s.history(&s.manager, "adjustment", &moved).await),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "the switch first"
    );
    s.grant_manager("record.history.read", true).await?;
    for id in [&moved, &own] {
        assert_eq!(
            s.history(&s.manager, "adjustment", id).await.status,
            StatusCode::NOT_FOUND,
            "{id}"
        );
    }
    let r = s.history(&s.manager, "adjustment", &third).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["summary"]["edits"], 2);
    let owners = s.owner_history("adjustment", &moved).await;
    assert_eq!(owners["summary"]["edits"], 1);
    // A removal is no edit; the entry reads as removed.
    let fine = record(
        &s.owner,
        json!({"staff_id": s.lan, "kind": "penalty", "amount": 50_000, "reason": "Đi muộn",
               "occurred_on": day}),
    )
    .await;
    let removed = call(
        &s.h.app,
        "DELETE",
        &format!("/adjustments/{fine}"),
        Some(&s.owner),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{}", removed.body);
    let story = s.owner_history("adjustment", &fine).await;
    assert_eq!(
        actions(&story),
        ["staff.adjustment", "staff.adjustment_void"]
    );
    assert_eq!(
        (&story["summary"]["edits"], &story["summary"]["state"]),
        (&json!(0), &json!("voided"))
    );
    assert_eq!(item(&story, "staff.adjustment")["detail"], "penalty");
    // Rights and ids: the technician 403 whatever the id; the owner 400 / 404.
    for junk in ["not-a-uuid", third.as_str()] {
        assert_eq!(
            s.history(&s.technician, "adjustment", junk).await.status,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        s.history(&s.owner, "adjustment", "not-a-uuid").await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        s.history(&s.owner, "adjustment", &Uuid::now_v7().to_string())
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    Ok(())
}

// --------------------------------------------------------- wave 3: the catalogue, the settings

impl Spa {
    /// `GET /record-history/settings/{which}` (`payroll-rules`, `appointment-settings`).
    async fn settings_history(&self, who: &str, which: &str) -> Reply {
        call(
            &self.h.app,
            "GET",
            &format!("/record-history/settings/{which}"),
            Some(who),
            None,
        )
        .await
    }

    async fn put_versioned(&self, who: &str, path: &str, version: i64, body: Value) -> Reply {
        let etag = format!("\"{version}\"");
        call_with_headers(
            &self.h.app,
            "PUT",
            path,
            Some(who),
            Some(body),
            &[("If-Match", &etag)],
        )
        .await
    }

    /// The seeded group "Gội đầu dưỡng sinh".
    async fn shampoo_group(&self) -> Setup<String> {
        let id: Uuid = sqlx::query_scalar(
            "SELECT id FROM app.service_category WHERE tenant_id = $1 AND name = 'Gội đầu dưỡng sinh'",
        )
        .bind(self.tenant)
        .fetch_one(&self.h.owner)
        .await?;
        Ok(id.to_string())
    }
}

/// The line of an action that changed this field.
fn change_of<'a>(history: &'a Value, action: &str, field: &str) -> &'a Value {
    history["items"]
        .as_array()
        .and_then(|i| {
            i.iter()
                .filter(|e| e["action"] == action)
                .flat_map(|e| e["changes"].as_array().into_iter().flatten())
                .find(|c| c["field"] == field)
        })
        .unwrap_or_else(|| panic!("no {action} changing {field} in {history}"))
}

/// Spec §7 (wave 3), AC "chung", K1: the catalogue's and the settings' histories are their screens'
/// — `catalog.manage`, `payroll.read`, `appointment.settings`, the owner's by default — so a
/// manager the operator showed the history to still gets `403` (whatever the id, malformed or
/// another spa's: no oracle); another spa's records are not found; a switched-off feature
/// answers `403 feature.disabled` from the route guard.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_catalogue_and_the_settings_are_the_owners_alone(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let group = s.shampoo_group().await?;
    let records = [
        ("service", s.shampoo.clone()),
        ("package-plan", s.plan.clone()),
        ("service-category", group),
    ];
    let settings = ["payroll-rules", "appointment-settings"];
    let no_store = |r: &Reply| {
        r.headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    for (kind, id) in &records {
        let r = s.history(&s.owner, kind, id).await;
        assert_eq!(r.status, StatusCode::OK, "{kind}: {}", r.body);
        assert_eq!(no_store(&r).as_deref(), Some("no-store"), "{kind}");
    }
    for which in settings {
        let r = s.settings_history(&s.owner, which).await;
        assert_eq!(r.status, StatusCode::OK, "{which}: {}", r.body);
        assert_eq!(no_store(&r).as_deref(), Some("no-store"), "{which}");
        assert!(r.body["record"].get("id").is_none(), "{which}: {}", r.body);
    }

    // Shown the history by the operator, the manager still lacks the screens' rights; the
    // technician never has the switch. Junk ids are refused the same way (K1).
    s.grant_manager("record.history.read", true).await?;
    let unknown = Uuid::now_v7().to_string();
    for who in [&s.manager, &s.technician] {
        for (kind, id) in &records {
            for junk in [id.as_str(), "not-a-uuid", unknown.as_str()] {
                assert_eq!(
                    code(&s.history(who, kind, junk).await),
                    (StatusCode::FORBIDDEN, Some("auth.forbidden")),
                    "{kind} {junk}"
                );
            }
        }
        for which in settings {
            assert_eq!(
                code(&s.settings_history(who, which).await),
                (StatusCode::FORBIDDEN, Some("auth.forbidden")),
                "{which}"
            );
        }
    }
    // The owner: malformed → 400, unknown → 404.
    for (kind, _) in &records {
        assert_eq!(
            s.history(&s.owner, kind, "not-a-uuid").await.status,
            StatusCode::BAD_REQUEST,
            "{kind}"
        );
        assert_eq!(
            s.history(&s.owner, kind, &unknown).await.status,
            StatusCode::NOT_FOUND,
            "{kind}"
        );
    }

    // Another spa's owner: this spa's service, plan and group are not found; its own settings
    // are its own (this spa's save is not in them).
    let saved = call(
        &s.h.app,
        "PUT",
        "/appointment-settings",
        Some(&s.owner),
        Some(json!({"max_per_slot": 4})),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    create_tenant(&s.h.owner, "khac", "Spa Khác", "Huế").await?;
    add_member(
        &s.h.owner,
        "khac",
        OTHER_OWNER_PHONE,
        "Chủ Khác",
        SystemRole::Owner,
    )
    .await?;
    let other = session_for(&s.h, OTHER_OWNER_PHONE).await;
    for (kind, id) in &records {
        assert_eq!(
            s.history(&other, kind, id).await.status,
            StatusCode::NOT_FOUND,
            "{kind}"
        );
    }
    let theirs = s.settings_history(&other, "appointment-settings").await;
    assert_eq!(theirs.status, StatusCode::OK, "{}", theirs.body);
    assert_eq!(theirs.body["items"], json!([]));
    assert_eq!(
        s.settings_history(&s.owner, "appointment-settings")
            .await
            .body["items"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    // A switched-off feature: the route guard answers, before any handler (spec edge case 12).
    s.switch_off("payroll").await?;
    assert_eq!(
        code(&s.settings_history(&s.owner, "payroll-rules").await),
        (StatusCode::FORBIDDEN, Some("feature.disabled"))
    );
    assert_eq!(
        s.settings_history(&s.owner, "appointment-settings")
            .await
            .status,
        StatusCode::OK
    );
    s.switch_off("appointments").await?;
    assert_eq!(
        code(&s.settings_history(&s.owner, "appointment-settings").await),
        (StatusCode::FORBIDDEN, Some("feature.disabled"))
    );
    // The catalogue has no feature switch of its own.
    assert_eq!(
        s.history(&s.owner, "service", &s.shampoo).await.status,
        StatusCode::OK
    );
    Ok(())
}

/// Spec §4.1 (wave 3), edge case 9, design §2.10: a service, a plan and a group of services say
/// who added and who edited them, an edit names old → new values ("Giá Nữ: 150.000đ → 170.000đ",
/// a renamed group from its previous row), only edits count ("đã sửa N lần": the creation is
/// none), and what the spa was seeded with has no trail ("Chưa rõ ai ghi").
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_catalogue_says_who_added_and_who_edited(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let group = s.shampoo_group().await?;

    // Seeded with the spa: no line, nobody.
    let seeded = s.owner_history("service", &s.cheaper).await;
    assert_eq!(
        seeded["record"],
        json!({"type": "service", "id": s.cheaper})
    );
    assert_eq!(seeded["items"], json!([]));
    assert_eq!(seeded["summary"], json!({"edits": 0, "state": "active"}));

    // A service added, then edited twice by gender prices.
    let service = |female: i64, male: i64, recommended: bool| {
        json!({"category_id": group, "name": "Gội thảo dược", "description": null,
               "duration_min": 60, "recommended": recommended, "active": true,
               "prices": {"female": female, "male": male}})
    };
    let made = call(
        &s.h.app,
        "POST",
        "/services",
        Some(&s.owner),
        Some(service(150_000, 150_000, false)),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let id = str_of(&made.body["id"]);
    let path = format!("/services/{id}");
    let added = s.owner_history("service", &id).await;
    assert_eq!(actions(&added), ["catalog.service_create"]);
    assert_eq!(item(&added, "catalog.service_create")["changes"], json!([]));
    assert_eq!(
        (
            &added["summary"]["edits"],
            &added["summary"]["created"]["actor"]["name"],
            &added["summary"]["created"]["source"]
        ),
        (&json!(0), &json!("Chủ Spa"), &json!("audit"))
    );
    assert!(added["summary"].get("last").is_none(), "{added}");

    let edited = s
        .put_versioned(&s.owner, &path, 1, service(170_000, 150_000, false))
        .await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    let once = s.owner_history("service", &id).await;
    assert_eq!(
        actions(&once),
        ["catalog.service_create", "catalog.service_update"]
    );
    assert_eq!(
        item(&once, "catalog.service_update")["changes"],
        json!([{"field": "price_female", "format": "money", "old": "150000", "new": "170000"}])
    );
    assert_eq!(once["summary"]["edits"], 1);
    assert_eq!(once["summary"]["last"]["actor"]["name"], "Chủ Spa");

    let again = s
        .put_versioned(
            &s.owner,
            &path,
            edited.body["version"].as_i64().unwrap_or_default(),
            service(170_000, 160_000, true),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.body);
    let twice = s.owner_history("service", &id).await;
    assert_eq!(twice["summary"]["edits"], 2);
    assert_eq!(
        change_of(&twice, "catalog.service_update", "price_male"),
        &json!({"field": "price_male", "format": "money", "old": "150000", "new": "160000"})
    );
    assert_eq!(
        change_of(&twice, "catalog.service_update", "recommended")["new"],
        "true"
    );
    // Moved to another group: by name, not by id.
    let other_group = call(
        &s.h.app,
        "POST",
        "/service-categories",
        Some(&s.owner),
        Some(json!({"name": "Chăm sóc tay chân"})),
    )
    .await;
    assert_eq!(
        other_group.status,
        StatusCode::CREATED,
        "{}",
        other_group.body
    );
    let mut moved = service(170_000, 160_000, true);
    moved["category_id"] = other_group.body["id"].clone();
    let moved = s
        .put_versioned(
            &s.owner,
            &path,
            again.body["version"].as_i64().unwrap_or_default(),
            moved,
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(
        change_of(
            &s.owner_history("service", &id).await,
            "catalog.service_update",
            "category"
        ),
        &json!({"field": "category", "format": "text",
                "old": "Gội đầu dưỡng sinh", "new": "Chăm sóc tay chân"})
    );

    // The plan the owner made for every test: its price edited once.
    let plan = s.owner_history("package-plan", &s.plan).await;
    assert_eq!(actions(&plan), ["catalog.plan_create"]);
    assert_eq!(plan["summary"]["edits"], 0);
    let repriced = s
        .put_versioned(
            &s.owner,
            &format!("/package-plans/{}", s.plan),
            1,
            json!({
                "name": PLAN_NAME, "price": 1_800_000, "validity_months": 6,
                "gift_note": null, "active": true,
                "items": [{"service_id": s.shampoo, "sessions": 10, "bonus_sessions": 2}],
            }),
        )
        .await;
    assert_eq!(repriced.status, StatusCode::OK, "{}", repriced.body);
    let plan = s.owner_history("package-plan", &s.plan).await;
    assert_eq!(
        plan["record"],
        json!({"type": "package_plan", "id": s.plan})
    );
    assert_eq!(
        item(&plan, "catalog.plan_update")["changes"],
        json!([{"field": "price", "format": "money", "old": "1500000", "new": "1800000"}])
    );
    assert_eq!(plan["summary"]["edits"], 1);

    // A group: its writer keeps no old values, its previous row has them.
    let new_group = str_of(&other_group.body["id"]);
    let order = other_group.body["sort_order"].clone();
    let rename = |name: &str, order: &Value| {
        let (s, path, body) = (
            &s,
            format!("/service-categories/{new_group}"),
            json!({"name": name, "sort_order": order}),
        );
        async move {
            let r = call(&s.h.app, "PUT", &path, Some(&s.owner), Some(body)).await;
            assert_eq!(r.status, StatusCode::OK, "{}", r.body);
        }
    };
    rename("Chăm sóc tay và chân", &order).await;
    rename("Chăm sóc tay và chân", &json!(90)).await;
    let story = s.owner_history("service-category", &new_group).await;
    assert_eq!(
        story["record"],
        json!({"type": "service_category", "id": new_group})
    );
    assert_eq!(story["summary"]["edits"], 2);
    assert_eq!(
        change_of(&story, "catalog.category_update", "name"),
        &json!({"field": "name", "format": "text",
                "old": "Chăm sóc tay chân", "new": "Chăm sóc tay và chân"})
    );
    assert_eq!(
        change_of(&story, "catalog.category_update", "sort_order")["new"],
        "90"
    );
    assert_eq!(
        lines(&story)
            .iter()
            .filter(|(a, _)| a == "catalog.category_update")
            .map(|(_, fields)| fields.clone())
            .collect::<Vec<_>>(),
        [vec!["name".to_owned()], vec!["sort_order".to_owned()]]
    );
    // A seeded group edited: no previous row, so the line says who and when, not what.
    let seeded = s.shampoo_group().await?;
    let r = call(
        &s.h.app,
        "PUT",
        &format!("/service-categories/{seeded}"),
        Some(&s.owner),
        Some(json!({"name": "Gội đầu dưỡng sinh", "sort_order": 7})),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    let story = s.owner_history("service-category", &seeded).await;
    assert_eq!(
        lines(&story),
        [("catalog.category_update".to_owned(), Vec::new())]
    );
    assert!(story["summary"].get("created").is_none(), "{story}");
    assert_eq!(story["summary"]["edits"], 1);
    assert_eq!(story["summary"]["last"]["actor"]["name"], "Chủ Spa");
    Ok(())
}

/// Spec §4.1 (wave 3), architecture §4.4: the payroll rules and the calendar's "số khách tối đa"
/// are the spa's own (no id). Never saved: no line. Every save is a line with what it changed,
/// the first one stands for "created", every later one is an edit.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_spa_settings_count_every_save_after_the_first(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    for which in ["payroll-rules", "appointment-settings"] {
        let r = s.settings_history(&s.owner, which).await;
        assert_eq!(r.status, StatusCode::OK, "{which}: {}", r.body);
        assert_eq!(r.body["items"], json!([]), "{which}");
        assert_eq!(
            r.body["summary"],
            json!({"edits": 0, "state": "active"}),
            "{which}"
        );
    }

    // The payroll rules, saved twice (each save replaces them, missing keys at their defaults).
    let first = s
        .put_versioned(
            &s.owner,
            "/payroll/rules",
            0,
            json!({"late_grace_minutes": 10}),
        )
        .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let rules = s.settings_history(&s.owner, "payroll-rules").await.body;
    assert_eq!(rules["record"], json!({"type": "payroll_rules"}));
    assert_eq!(
        (
            &rules["summary"]["edits"],
            &rules["summary"]["created"]["actor"]["name"]
        ),
        (&json!(0), &json!("Chủ Spa"))
    );
    assert_eq!(
        item(&rules, "payroll.rules_update")["changes"],
        json!([{"field": "late_grace_minutes", "format": "number", "old": "0", "new": "10"}])
    );
    let second = s
        .put_versioned(
            &s.owner,
            "/payroll/rules",
            1,
            json!({"late_grace_minutes": 15, "late_penalty_amount": 30_000,
                   "loyalty_partial_month": "prorate"}),
        )
        .await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    let rules = s.settings_history(&s.owner, "payroll-rules").await.body;
    assert_eq!(rules["summary"]["edits"], 1);
    assert_eq!(
        actions(&rules),
        ["payroll.rules_update", "payroll.rules_update"]
    );
    assert_eq!(
        change_of(&rules, "payroll.rules_update", "late_penalty_amount"),
        &json!({"field": "late_penalty_amount", "format": "money", "old": "0", "new": "30000"})
    );
    assert_eq!(
        change_of(&rules, "payroll.rules_update", "loyalty_partial_month"),
        &json!({"field": "loyalty_partial_month", "format": "code",
                "old": "fifteen_days", "new": "prorate"})
    );

    // "Số khách tối đa": 4, then 6, then no limit.
    for max in [json!(4), json!(6), Value::Null] {
        let r = call(
            &s.h.app,
            "PUT",
            "/appointment-settings",
            Some(&s.owner),
            Some(json!({"max_per_slot": max})),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    }
    let slots = s
        .settings_history(&s.owner, "appointment-settings")
        .await
        .body;
    assert_eq!(slots["record"], json!({"type": "appointment_settings"}));
    assert_eq!(slots["summary"]["edits"], 2);
    let mut changes: Vec<Value> = slots["items"]
        .as_array()
        .map(|i| i.iter().map(|e| e["changes"][0].clone()).collect())
        .unwrap_or_default();
    changes.sort_by_key(Value::to_string);
    let mut expected = vec![
        json!({"field": "max_per_slot", "format": "number", "new": "4"}),
        json!({"field": "max_per_slot", "format": "number", "old": "4", "new": "6"}),
        json!({"field": "max_per_slot", "format": "number", "old": "6"}),
    ];
    expected.sort_by_key(Value::to_string);
    assert_eq!(changes, expected);
    Ok(())
}

// ------------------------------------------------------------------- the order of the lines

/// The actions of the lines in the order the history gives them (oldest first).
fn in_order(history: &Value) -> Vec<String> {
    history["items"]
        .as_array()
        .map(|i| i.iter().map(|e| str_of(&e["action"])).collect())
        .unwrap_or_default()
}

/// The owner as a use case sees them, to write the way a request does.
async fn owner_actor(s: &Spa, app: &PgPool) -> Setup<Actor> {
    let e164 = kernel::PhoneE164::parse(OWNER_PHONE).map_err(|_| "phone")?;
    let user: Uuid = sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
        .bind(e164.as_str())
        .fetch_one(&s.h.owner)
        .await?;
    let (t, u) = (TenantId::from_uuid(s.tenant), UserId::from_uuid(user));
    let permissions = identity::service::tenant_permissions(app, u, t)
        .await?
        .ok_or("not a member")?;
    Ok(Actor::new(u, t, permissions, None))
}

/// K60 / ADR-0023: request 1 began first but wrote later (it waited on the row lock request 2
/// held), so its line reads after request 2's and a group's renames chain "A → B", "B → C" as the
/// group went. Stamped with the transaction's start, request 1's line came first: "A → C",
/// "C → B" (the name read last was not the group's). Request 2 is the route; request 1 writes as
/// `catalog::commands::update_category` does, in a transaction opened before request 2.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn racing_edits_read_in_the_order_they_were_made(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let made = call(
        &s.h.app,
        "POST",
        "/service-categories",
        Some(&s.owner),
        Some(json!({"name": "Nhóm A"})),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let group = Uuid::parse_str(&str_of(&made.body["id"]))?;
    let order = made.body["sort_order"].clone();
    let app = app_pool(&s.h.owner).await?;
    let owner = owner_actor(&s, &app).await?;

    let mut first = TenantTx::for_actor(&app, &owner).await?;
    let second = call(
        &s.h.app,
        "PUT",
        &format!("/service-categories/{group}"),
        Some(&s.owner),
        Some(json!({"name": "Nhóm B", "sort_order": order})),
    )
    .await;
    assert_eq!(second.status, StatusCode::OK, "{}", second.body);
    sqlx::query("UPDATE app.service_category SET name = 'Nhóm C' WHERE id = $1")
        .bind(group)
        .execute(first.conn())
        .await?;
    audit::record(
        &mut first,
        &owner,
        AuditEntry {
            action: Action::CatalogCategoryUpdate,
            entity_type: "service_category",
            entity_id: group,
            before: None,
            after: Some(json!({"name": "Nhóm C", "sort_order": order})),
            reason: None,
        },
    )
    .await?;
    first.commit().await?;

    let story = s
        .owner_history("service-category", &group.to_string())
        .await;
    assert_eq!(
        in_order(&story),
        [
            "catalog.category_create",
            "catalog.category_update",
            "catalog.category_update"
        ]
    );
    let renames: Vec<(&Value, &Value)> = story["items"]
        .as_array()
        .map(|i| {
            i.iter()
                .flat_map(|e| e["changes"].as_array().into_iter().flatten())
                .filter(|c| c["field"] == "name")
                .map(|c| (&c["old"], &c["new"]))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        renames,
        [
            (&json!("Nhóm A"), &json!("Nhóm B")),
            (&json!("Nhóm B"), &json!("Nhóm C"))
        ],
        "{story}"
    );
    assert_eq!(story["summary"]["edits"], 2);
    Ok(())
}

/// K60 / ADR-0023: a request's lines are stamped by the database clock when it writes, never by
/// the clock its handler read on arrival (the `now` the routes pass to the `*_as_of` use cases):
/// "Sửa lượt" whose handler started before a technician change committed reads after that change,
/// as it was made after it. The replies still count their own edit (`edited`).
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_request_reads_when_it_wrote_not_when_it_arrived(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let app = app_pool(&s.h.owner).await?;
    let owner = owner_actor(&s, &app).await?;
    let visit = s.simple_visit().await;
    let id = str_of(&visit.body["id"]);
    let line = str_of(&visit.body["lines"][0]["id"]);

    let arrived = Utc::now();
    let moved = s
        .patch(
            &s.owner,
            &id,
            1,
            json!({"lines": [{"id": line, "staff_id": s.mai}]}),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    assert_eq!(moved.body["edited"], true, "the reply counts its own edit");
    let fixed: VisitCreate = serde_json::from_value(json!({
        "customer_id": s.ha, "lines": [s.one_off(&s.cheaper)],
        "payments": [{"method": "cash", "amount": 39_000}],
    }))?;
    let k = Uuid::now_v7();
    let (_, corrected) = operations::service::correct_as_of(
        &app,
        &s.h.cipher,
        &owner,
        k,
        hash_request(k.as_bytes()),
        operations::service::Correction {
            id: Uuid::parse_str(&id)?,
            version: moved.body["version"].as_i64().unwrap_or_default(),
            reason: Some("ghi nhầm dịch vụ"),
            note: None,
            visit: &fixed,
        },
        arrived,
    )
    .await?;
    assert_eq!(corrected["visit"]["edited"], true, "{corrected}");

    let story = s.owner_history("visit", &id).await;
    assert_eq!(
        in_order(&story),
        ["visit.create", "visit.update", "visit.correct"],
        "{story}"
    );
    let stamped: Vec<chrono::DateTime<Utc>> = sqlx::query_scalar(
        "SELECT at FROM app.audit_log WHERE action = 'visit.correct' AND entity_id = $1",
    )
    .bind(Uuid::parse_str(&id)?)
    .fetch_all(&s.h.owner)
    .await?;
    assert!(
        stamped.iter().all(|at| *at > arrived),
        "{stamped:?} vs {arrived}"
    );
    Ok(())
}
