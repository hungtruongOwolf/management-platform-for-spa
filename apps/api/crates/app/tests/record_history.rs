//! "Ai ghi, ai sửa" on each record (`GET /record-history/...`, ADR-0022, spec record-history):
//! who may read it and in which order the rights are checked (AC-H1, AC-H4), what belongs to a
//! visit's and a package's history (D6, AC-H2), "Sửa lượt" read as one story (AC-H3), what counts
//! as an edit and the lists' `edited` agreeing with it (D9, D10, AC-H8), what each caller may see
//! (D7, D8, AC-H7) and where "Ghi bởi" comes from (D12).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::{FixedOffset, NaiveDate, Utc};
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
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

fn actions(history: &Value) -> Vec<String> {
    history["items"]
        .as_array()
        .map(|i| i.iter().map(|e| str_of(&e["action"])).collect())
        .unwrap_or_default()
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
            "sale.create",
            "customer_package.extend",
            "customer_package.cancel"
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
            actions(story),
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
    assert_eq!(actions(&upsell), ["visit.create", "visit.addition"]);
    let resend = s.owner_history("visit", &resent_id).await;
    assert_eq!(actions(&resend), ["visit.create", "visit.update"]);
    let technician = s.owner_history("visit", &moved_id).await;
    assert_eq!(
        technician["items"][1]["changes"],
        json!([{"field": "staff", "format": "text", "old": "KTV Lan", "new": "KTV Mai"}])
    );
    let session_note = s.owner_history("visit", &noted_id).await;
    assert_eq!(
        session_note["items"][1]["changes"],
        json!([{"field": "session_note", "format": "flag", "new": "true"}])
    );
    assert!(!session_note.to_string().contains("Đau vai gáy"));
    let conversion_story = s.owner_history("visit", &converted_id).await;
    assert_eq!(
        actions(&conversion_story),
        ["visit.create", "visit.convert_to_package"]
    );
    assert_eq!(conversion_story["items"][1]["packages"], json!([PLAN_NAME]));

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
    let mut seen = actions(&story);
    seen.sort();
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
    let changes = owner["items"][1]["changes"]
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
    let changes = manager.body["items"][1]["changes"]
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
    assert_eq!(actions(&story), ["visit.create", "sale.create"], "{story}");
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
                    "starts_on": "2026-06-01", "expires_on": null,
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
        ["opening_balance.import", "customer_package.extend"],
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
