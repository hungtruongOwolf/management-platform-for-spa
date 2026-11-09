//! "Ghi chú gói" (F4, EM 2026-11-08): one optional note per package sale. Encrypted at rest under
//! its own sale, written only with `customer.health_notes.write`, read back only by
//! `customer.health_notes.read` (key absent otherwise), never in an idempotent response, audit or
//! a technician's / former staff member's view. Written by `POST /sales`, each package of
//! `POST /sales/checkout`, each package sold in `POST /visits` and `/visits/{id}/additions`;
//! edited by `PATCH /sales/{id}`; shown in Quầy's day lists, the customer timeline, the package
//! page and "Nhật ký ghi chú" (`package_note`).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use chrono::Utc;
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use kernel::testing::app_pool;
use kernel::{Actor, AppError, Permission as P, TenantId, UserId};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const PRICE: i64 = 1_500_000;

const NOTE_A: &str = "Đau vai gáy, xin ấn nhẹ";
const NOTE_B: &str = "Mẹ bị huyết áp cao, tránh xông nóng";
const SESSION_NOTE: &str = "Đỏ da vùng cổ sau buổi";
const VISIT_PACKAGE_NOTE: &str = "Gói này cho đợt trị liệu lưng";
const ADDED_PACKAGE_NOTE: &str = "Gói thêm: khách có thai 3 tháng";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Desk {
    h: Harness,
    owner: String,
    manager: String,
    tech: String,
    tenant: Uuid,
    /// A real account to act as (who-did-it columns reference user_account, 0036).
    user: Uuid,
    /// The payer (Nguyễn Thị Hà) and the person she buys for (Trần Thị Mẹ).
    ha: String,
    mother: String,
    plan: String,
    shampoo: String,
    /// KTV Mai's staff profile, linked to the technician login.
    mai: String,
}

fn id_of(reply: &Reply) -> String {
    reply.body["id"].as_str().unwrap_or_default().to_owned()
}

fn today() -> chrono::NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

async fn desk(pool: PgPool) -> Setup<Desk> {
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
        TECH_PHONE,
        "KTV Mai",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let services = call(&h.app, "GET", "/services", Some(&owner), None).await;
    let shampoo = services
        .body
        .as_array()
        .and_then(|l| l.iter().find(|s| s["name"] == "Cơ Bản – Dầu thảo dược"))
        .and_then(|s| s["id"].as_str())
        .unwrap_or_default()
        .to_owned();
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu dưỡng sinh 10 buổi", "price": PRICE, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 0}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let mut people = Vec::new();
    for (name, phone) in [
        ("Nguyễn Thị Hà", "0905111222"),
        ("Trần Thị Mẹ", "0905333444"),
    ] {
        let c = call(
            &h.app,
            "POST",
            "/customers",
            Some(&owner),
            Some(json!({"full_name": name, "phone": phone, "gender": "female"})),
        )
        .await;
        assert_eq!(c.status, StatusCode::CREATED, "{}", c.body);
        people.push(id_of(&c));
    }
    let tech_user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = '+84912000222'")
            .fetch_one(&h.owner)
            .await?;
    let mai = call(
        &h.app,
        "POST",
        "/staff",
        Some(&owner),
        Some(json!({"full_name": "KTV Mai", "position": "Kỹ thuật viên", "user_id": tech_user})),
    )
    .await;
    assert_eq!(mai.status, StatusCode::CREATED, "{}", mai.body);
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account ORDER BY created_at, id LIMIT 1")
            .fetch_one(&h.owner)
            .await?;
    Ok(Desk {
        owner,
        manager,
        tech,
        tenant,
        user,
        mother: people.pop().unwrap_or_default(),
        ha: people.pop().unwrap_or_default(),
        plan: id_of(&plan),
        shampoo,
        mai: id_of(&mai),
        h,
    })
}

impl Desk {
    async fn post(&self, session: &str, path: &str, body: Value) -> Reply {
        self.post_key(session, path, body, &Uuid::now_v7().to_string())
            .await
    }

    async fn post_key(&self, session: &str, path: &str, body: Value, key: &str) -> Reply {
        call_with_headers(
            &self.h.app,
            "POST",
            path,
            Some(session),
            Some(body),
            &[("idempotency-key", key)],
        )
        .await
    }

    async fn get(&self, session: &str, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(session), None).await
    }

    async fn patch(&self, session: &str, path: &str, body: Value) -> Reply {
        call(&self.h.app, "PATCH", path, Some(session), Some(body)).await
    }

    fn sale(&self, note: Option<&str>) -> Value {
        json!({
            "customer_id": self.ha, "closer_staff_id": null,
            "lines": [{"package_plan_id": self.plan}],
            "payments": [{"method": "cash", "amount": PRICE}],
            "note": note,
        })
    }

    fn actor(&self, permissions: &[P]) -> Actor {
        Actor::new(
            UserId::from_uuid(self.user),
            TenantId::from_uuid(self.tenant),
            permissions.iter().copied(),
            None,
        )
    }

    /// A former manager (ADR-0014): only the derived former-staff permissions.
    fn former(&self) -> Actor {
        let perms = kernel::permission::effective_permissions(
            SystemRole::Manager.default_permissions(),
            true,
        );
        Actor::new(
            UserId::from_uuid(self.user),
            TenantId::from_uuid(self.tenant),
            perms,
            None,
        )
    }

    /// The seeded manager minus `without`.
    fn manager_without(&self, without: P) -> Actor {
        let perms: Vec<P> = SystemRole::Manager
            .default_permissions()
            .into_iter()
            .filter(|p| *p != without)
            .collect();
        self.actor(&perms)
    }

    /// No row of the audit log or of the (unencrypted) idempotency table holds `text`.
    async fn leaks(&self, text: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM app.audit_log WHERE after::text LIKE '%' || $1 || '%'
                                                         OR before::text LIKE '%' || $1 || '%')
                  + (SELECT count(*) FROM app.idempotency_record WHERE response_body::text LIKE '%' || $1 || '%')",
        )
        .bind(text)
        .fetch_one(&self.h.owner)
        .await
    }

    async fn note_ct(&self, sale: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        Ok(
            sqlx::query_scalar("SELECT note_ct FROM app.sale WHERE id = $1")
                .bind(Uuid::parse_str(sale)?)
                .fetch_one(&self.h.owner)
                .await?,
        )
    }
}

fn history_query() -> reporting::dto::HistoryQuery {
    reporting::dto::HistoryQuery {
        q: None,
        kind: None,
        from: None,
        to: None,
        cursor: None,
        limit: None,
    }
}

/// The purchase event of `sale` on a timeline body.
fn purchase_of(history: &Value, sale: &str) -> Value {
    history["events"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["purchase"]["sale_id"] == sale)
        .cloned()
        .unwrap_or_default()
}

fn forbidden<T: std::fmt::Debug>(r: &Result<T, AppError>) -> bool {
    matches!(r, Err(AppError::Forbidden { .. }))
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_package_sale_note_is_encrypted_and_read_only_by_health_readers(
    pool: PgPool,
) -> TestResult {
    let d = desk(pool).await?;
    let key = Uuid::now_v7().to_string();
    let sold = d
        .post_key(
            &d.manager,
            "/sales",
            d.sale(Some(&format!("  {NOTE_A}  "))),
            &key,
        )
        .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    assert_eq!(
        sold.body.get("note"),
        None,
        "the stored response never holds the note"
    );
    let sale = id_of(&sold);
    let replay = d
        .post_key(
            &d.manager,
            "/sales",
            d.sale(Some(&format!("  {NOTE_A}  "))),
            &key,
        )
        .await;
    assert_eq!(replay.body, sold.body, "a replay is the first response");

    // At rest: ciphertext only, nothing in audit or the idempotency table.
    let ct = d.note_ct(&sale).await?.unwrap_or_default();
    assert!(!ct.is_empty());
    assert!(!String::from_utf8_lossy(&ct).contains("vai gáy"));
    assert_eq!(d.leaks("vai gáy").await?, 0);
    let audited: Option<String> = sqlx::query_scalar(
        "SELECT after->>'note' FROM app.audit_log WHERE action = 'sale.create' AND entity_id = $1",
    )
    .bind(Uuid::parse_str(&sale)?)
    .fetch_one(&d.h.owner)
    .await?;
    assert_eq!(audited.as_deref(), Some(kernel::audit::REDACTED));

    // Read back, trimmed: the sale, Quầy's day list, the timeline, the package page.
    let one = d.get(&d.manager, &format!("/sales/{sale}")).await;
    assert_eq!(one.body["note"], NOTE_A, "{}", one.body);
    let day = d.get(&d.manager, "/sales").await;
    assert_eq!(day.body[0]["sale"]["note"], NOTE_A, "{}", day.body);
    let history = d
        .get(&d.manager, &format!("/customers/{}/history", d.ha))
        .await;
    assert_eq!(history.status, StatusCode::OK, "{}", history.body);
    assert_eq!(
        purchase_of(&history.body, &sale)["purchase"]["note"],
        NOTE_A
    );
    let package = sold.body["customer_packages"][0]["id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let usage = d
        .get(&d.manager, &format!("/customer-packages/{package}/usage"))
        .await;
    assert_eq!(usage.status, StatusCode::OK, "{}", usage.body);
    assert_eq!(usage.body["sale_note"], NOTE_A);
    // "Gói liệu trình của khách": the list and one package carry it; the sale's own package does not.
    let list = d
        .get(&d.manager, &format!("/customers/{}/packages", d.ha))
        .await;
    let listed = list
        .body
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == package.as_str())
        .cloned()
        .unwrap_or_default();
    assert_eq!(listed["sale_note"], NOTE_A, "{}", list.body);
    let one_package = d
        .get(&d.manager, &format!("/customer-packages/{package}"))
        .await;
    assert_eq!(one_package.body["sale_note"], NOTE_A);
    assert_eq!(sold.body["customer_packages"][0].get("sale_note"), None);

    // A sale without a note reads `null` for health readers.
    let plain = d.post(&d.manager, "/sales", d.sale(Some("   "))).await;
    assert_eq!(plain.status, StatusCode::CREATED, "{}", plain.body);
    assert_eq!(d.note_ct(&id_of(&plain)).await?, None, "blank = no note");
    let plain_read = d
        .get(&d.manager, &format!("/sales/{}", id_of(&plain)))
        .await;
    assert_eq!(plain_read.body.get("note"), Some(&Value::Null));

    // Without customer.health_notes.read the key is absent everywhere.
    let app = app_pool(&d.h.owner).await?;
    let blind = d.manager_without(P::CustomerHealthNotesRead);
    let sale_id = Uuid::parse_str(&sale)?;
    let seen = sales::service::get_sale(&app, &d.h.cipher, &blind, sale_id).await?;
    assert_eq!(seen.note, None);
    assert!(serde_json::to_value(&seen)?.get("note").is_none());
    let listed = sales::service::for_day(&app, &d.h.cipher, &blind, None).await?;
    assert!(listed.iter().all(|s| s.sale.note.is_none()));
    assert!(!serde_json::to_string(&listed)?.contains("vai gáy"));
    let page = reporting::customer::history(
        &app,
        &d.h.cipher,
        &blind,
        Uuid::parse_str(&d.ha)?,
        &history_query(),
    )
    .await?;
    assert!(!serde_json::to_string(&page)?.contains("vai gáy"));
    assert!(
        page.events
            .iter()
            .filter_map(|e| e.purchase.as_ref())
            .all(|p| p.note.is_none())
    );
    let usage =
        reporting::customer::package_usage(&app, &d.h.cipher, &blind, Uuid::parse_str(&package)?)
            .await?;
    assert_eq!(usage.sale_note, None);
    assert!(
        packages::service::sale_notes(&app, &d.h.cipher, &blind, &[sale_id])
            .await?
            .is_empty()
    );

    // Technicians and former staff never reach it.
    for path in [
        format!("/sales/{sale}"),
        "/sales".to_owned(),
        format!("/customers/{}/history", d.ha),
        format!("/customer-packages/{package}/usage"),
        format!("/customer-packages/{package}"),
        format!("/customers/{}/packages", d.ha),
        "/notes".to_owned(),
    ] {
        let r = d.get(&d.tech, &path).await;
        assert_eq!(r.status, StatusCode::FORBIDDEN, "{path} {}", r.body);
    }
    let former = d.former();
    assert!(forbidden(
        &sales::service::get_sale(&app, &d.h.cipher, &former, sale_id).await
    ));
    assert!(forbidden(
        &sales::service::for_day(&app, &d.h.cipher, &former, None).await
    ));
    assert!(forbidden(
        &reporting::customer::history(
            &app,
            &d.h.cipher,
            &former,
            Uuid::parse_str(&d.ha)?,
            &history_query()
        )
        .await
    ));
    assert!(forbidden(
        &reporting::customer::package_usage(&app, &d.h.cipher, &former, Uuid::parse_str(&package)?)
            .await
    ));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn writing_a_package_note_needs_the_health_write_right(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let app = app_pool(&d.h.owner).await?;
    let sales_before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.sale")
        .fetch_one(&d.h.owner)
        .await?;
    let seller = d.manager_without(P::CustomerHealthNotesWrite);
    let with_note: sales::dto::SaleCreate = serde_json::from_value(d.sale(Some(NOTE_A)))?;
    let k = Uuid::now_v7();
    let refused = sales::service::create_sale(
        &app,
        &d.h.cipher,
        &seller,
        k,
        kernel::idempotency::hash_request(k.as_bytes()),
        &with_note,
    )
    .await;
    assert!(forbidden(&refused), "{refused:?}");
    // The checkout refuses the whole cart for one package's note.
    let cart: sales::dto::SaleCheckoutCreate = serde_json::from_value(json!({
        "customer_id": d.ha,
        "package_sales": [{"package_plan_id": d.plan}, {"package_plan_id": d.plan, "note": NOTE_B}],
        "payments": [{"method": "cash", "amount": 2 * PRICE}]
    }))?;
    let k = Uuid::now_v7();
    let refused = sales::service::create_checkout(
        &app,
        &d.h.cipher,
        &seller,
        k,
        kernel::idempotency::hash_request(k.as_bytes()),
        &cart,
    )
    .await;
    assert!(forbidden(&refused), "{refused:?}");
    let sales_after: i64 = sqlx::query_scalar("SELECT count(*) FROM app.sale")
        .fetch_one(&d.h.owner)
        .await?;
    assert_eq!(sales_after, sales_before, "nothing was sold");
    // The same seller sells fine without a note (only the note needs the right).
    let without: sales::dto::SaleCreate = serde_json::from_value(d.sale(None))?;
    let k = Uuid::now_v7();
    let (status, _) = sales::service::create_sale(
        &app,
        &d.h.cipher,
        &seller,
        k,
        kernel::idempotency::hash_request(k.as_bytes()),
        &without,
    )
    .await?;
    assert_eq!(status, 201);

    // Shape: at most 2000 characters, on the field that carries it.
    let long = "x".repeat(2001);
    let r = d.post(&d.manager, "/sales", d.sale(Some(&long))).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.body);
    assert_eq!(r.body["errors"][0]["field"], "note");
    assert_eq!(r.body["errors"][0]["code"], "too_long");
    let r = d
        .post(
            &d.manager,
            "/sales/checkout",
            json!({"customer_id": d.ha,
                   "package_sales": [{"package_plan_id": d.plan}, {"package_plan_id": d.plan, "note": long}],
                   "payments": [{"method": "cash", "amount": 2 * PRICE}]}),
        )
        .await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", r.body);
    assert_eq!(r.body["errors"][0]["field"], "package_sales[1].note");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_cart_of_packages_keeps_one_note_per_package(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let done = d
        .post(
            &d.manager,
            "/sales/checkout",
            json!({
                "customer_id": d.ha,
                "package_sales": [
                    {"package_plan_id": d.plan, "note": NOTE_A},
                    {"package_plan_id": d.plan, "beneficiary_customer_id": d.mother, "note": NOTE_B},
                    {"package_plan_id": d.plan}
                ],
                "payments": [{"method": "cash", "amount": 3 * PRICE}]
            }),
        )
        .await;
    assert_eq!(done.status, StatusCode::CREATED, "{}", done.body);
    let ids: Vec<String> = done.body["sales"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| s["id"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(ids.len(), 3);
    assert!(
        !done.body.to_string().contains("vai gáy") && !done.body.to_string().contains("huyết áp"),
        "the stored response never holds a note"
    );
    assert_eq!(d.leaks("vai gáy").await?, 0);
    assert_eq!(d.leaks("huyết áp").await?, 0);

    // Each sale its own note, its own ciphertext.
    let day = d.get(&d.manager, "/sales").await;
    let note_of = |sale: &str| -> Value {
        day.body
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r["sale"]["id"] == sale)
            .map(|r| r["sale"]["note"].clone())
            .unwrap_or_default()
    };
    assert_eq!(note_of(&ids[0]), NOTE_A, "{}", day.body);
    assert_eq!(note_of(&ids[1]), NOTE_B);
    assert_eq!(note_of(&ids[2]), Value::Null);
    let (a, b) = (
        d.note_ct(&ids[0]).await?.unwrap_or_default(),
        d.note_ct(&ids[1]).await?.unwrap_or_default(),
    );
    assert!(!String::from_utf8_lossy(&b).contains("huyết"));
    assert_eq!(d.note_ct(&ids[2]).await?, None);
    // The associated data binds a note to its sale: moved to another sale, it does not open.
    sqlx::query("UPDATE app.sale SET note_ct = $2 WHERE id = $1")
        .bind(Uuid::parse_str(&ids[2])?)
        .bind(&a)
        .execute(&d.h.owner)
        .await?;
    let moved = d.get(&d.manager, &format!("/sales/{}", ids[2])).await;
    assert_eq!(
        moved.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "{}",
        moved.body
    );
    assert!(!moved.body.to_string().contains("vai gáy"));
    sqlx::query("UPDATE app.sale SET note_ct = NULL WHERE id = $1")
        .bind(Uuid::parse_str(&ids[2])?)
        .execute(&d.h.owner)
        .await?;

    // The mother's timeline carries her package's note; "Nhật ký ghi chú" one entry per note,
    // about the package's owner.
    let mother = d
        .get(&d.manager, &format!("/customers/{}/history", d.mother))
        .await;
    assert_eq!(
        purchase_of(&mother.body, &ids[1])["purchase"]["note"],
        NOTE_B
    );
    let journal = d.get(&d.manager, "/notes?kind=package_note").await;
    assert_eq!(journal.status, StatusCode::OK, "{}", journal.body);
    let items = journal.body["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(items.len(), 2, "{}", journal.body);
    let entry = |text: &str| {
        items
            .iter()
            .find(|n| n["text"] == text)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(entry(NOTE_A)["customer_id"], json!(d.ha));
    assert_eq!(entry(NOTE_B)["customer_id"], json!(d.mother));
    assert_eq!(entry(NOTE_B)["customer_name"], "Trần Thị Mẹ");
    assert_eq!(entry(NOTE_B)["detail"], "Gội đầu dưỡng sinh 10 buổi");
    assert_eq!(entry(NOTE_B)["author_name"], "Quản lý Ngọc");
    // Searchable after decryption (accent-insensitive), like the session note.
    let found = d.get(&d.manager, "/notes?q=huyet%20ap%20cao").await;
    let found_kinds: Vec<&str> = found.body["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["kind"].as_str())
        .collect();
    assert_eq!(found_kinds, ["package_note"], "{}", found.body);
    // A different day shows none of it.
    let yesterday = today() - chrono::Duration::days(1);
    let other = d
        .get(
            &d.manager,
            &format!("/notes?kind=package_note&from={yesterday}&to={yesterday}"),
        )
        .await;
    assert_eq!(other.body["items"], json!([]));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_package_sold_in_a_visit_keeps_its_own_note(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let key = Uuid::now_v7().to_string();
    let visit = d
        .post_key(
            &d.manager,
            "/visits",
            json!({
                "customer_id": d.ha, "note": SESSION_NOTE,
                "lines": [{"service_id": d.shampoo, "source": "package", "package_ref": "p1",
                           "staff_id": d.mai}],
                "package_sales": [{"ref": "p1", "package_plan_id": d.plan, "note": VISIT_PACKAGE_NOTE}],
                "payments": [{"method": "cash", "amount": PRICE}]
            }),
            &key,
        )
        .await;
    assert_eq!(visit.status, StatusCode::CREATED, "{}", visit.body);
    let visit_id = id_of(&visit);
    assert_eq!(visit.body["package_sales"][0].get("note"), None);
    assert!(!visit.body.to_string().contains("trị liệu lưng"));

    // An addition sells one more package with its own note.
    let added = d
        .post(
            &d.manager,
            &format!("/visits/{visit_id}/additions"),
            json!({
                "package_sales": [{"package_plan_id": d.plan, "note": ADDED_PACKAGE_NOTE}],
                "payments": [{"method": "bank_transfer", "amount": PRICE}]
            }),
        )
        .await;
    assert_eq!(added.status, StatusCode::CREATED, "{}", added.body);
    assert!(!added.body.to_string().contains("có thai"));
    // Without customer.health_notes.write an addition with a note is refused.
    let app = app_pool(&d.h.owner).await?;
    let seller = d.manager_without(P::CustomerHealthNotesWrite);
    let input: operations::dto::VisitAdditions = serde_json::from_value(json!({
        "package_sales": [{"package_plan_id": d.plan, "note": "x"}],
        "payments": [{"method": "cash", "amount": PRICE}]
    }))?;
    let k = Uuid::now_v7();
    let refused = operations::service::add(
        &app,
        &d.h.cipher,
        &seller,
        k,
        kernel::idempotency::hash_request(k.as_bytes()),
        Uuid::parse_str(&visit_id)?,
        &input.into_visit(),
    )
    .await;
    assert!(forbidden(&refused), "{refused:?}");

    // The day list and the visit: the session note and each package's note side by side.
    let day = d.get(&d.manager, "/visits").await;
    let row = &day.body[0];
    assert_eq!(row["note"], SESSION_NOTE, "{}", day.body);
    assert_eq!(row["package_sales"][0]["note"], VISIT_PACKAGE_NOTE);
    assert_eq!(row["package_sales"][1]["note"], ADDED_PACKAGE_NOTE);
    let one = d.get(&d.manager, &format!("/visits/{visit_id}")).await;
    assert_eq!(one.body["package_sales"][0]["note"], VISIT_PACKAGE_NOTE);
    // The visit's package is a purchase on the timeline, with its note.
    let sale = row["package_sales"][0]["sale_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let history = d
        .get(&d.manager, &format!("/customers/{}/history", d.ha))
        .await;
    assert_eq!(
        purchase_of(&history.body, &sale)["purchase"]["note"],
        VISIT_PACKAGE_NOTE,
        "{}",
        history.body
    );
    for text in ["trị liệu lưng", "có thai"] {
        assert_eq!(d.leaks(text).await?, 0, "{text}");
    }

    // Without the health read right: no key on the visit's packages either.
    let blind = d.manager_without(P::CustomerHealthNotesRead);
    let seen = operations::service::for_day(&app, &d.h.cipher, &blind, None, None).await?;
    let hidden: Vec<bool> = seen
        .first()
        .map(|v| v.package_sales.iter().map(|p| p.note.is_none()).collect())
        .unwrap_or_default();
    assert_eq!(hidden, [true, true]);

    // The technician who served it sees the session note of their own visit (§14), never the
    // package's note.
    for path in [
        format!("/me/appointments?from={}&to={}", today(), today()),
        format!("/me/staff/day?date={}", today()),
    ] {
        let mine = d.get(&d.tech, &path).await;
        assert_eq!(mine.status, StatusCode::OK, "{path} {}", mine.body);
        let text = mine.body.to_string();
        assert!(text.contains(&visit_id), "{path} shows the visit: {text}");
        assert!(
            !text.contains("trị liệu lưng") && !text.contains("có thai"),
            "{path}: {text}"
        );
    }
    let journal = d.get(&d.manager, "/notes?kind=package_note").await;
    assert_eq!(
        journal.body["items"].as_array().map(Vec::len),
        Some(2),
        "{}",
        journal.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_desk_edits_a_package_note_in_place(pool: PgPool) -> TestResult {
    let d = desk(pool).await?;
    let sold = d.post(&d.manager, "/sales", d.sale(Some(NOTE_A))).await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    let sale = id_of(&sold);
    let path = format!("/sales/{sale}");

    let fixed = d.patch(&d.manager, &path, json!({"note": NOTE_B})).await;
    assert_eq!(fixed.status, StatusCode::OK, "{}", fixed.body);
    assert_eq!(fixed.body["note"], NOTE_B);
    assert_eq!(d.get(&d.manager, &path).await.body["note"], NOTE_B);
    let ct = d.note_ct(&sale).await?.unwrap_or_default();
    assert!(!String::from_utf8_lossy(&ct).contains("huyết"));
    let audited: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT after->>'note' FROM app.audit_log WHERE action = 'sale.note_update' ORDER BY at",
    )
    .fetch_all(&d.h.owner)
    .await?;
    assert_eq!(audited, [Some(kernel::audit::REDACTED.to_owned())]);
    assert_eq!(d.leaks("huyết áp").await?, 0);
    let cleared = d.patch(&d.manager, &path, json!({"note": "  "})).await;
    assert_eq!(cleared.status, StatusCode::OK, "{}", cleared.body);
    assert_eq!(cleared.body.get("note"), Some(&Value::Null));
    assert_eq!(d.note_ct(&sale).await?, None);

    // Shape and rights.
    let long = d
        .patch(&d.manager, &path, json!({"note": "x".repeat(2001)}))
        .await;
    assert_eq!(
        long.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        long.body
    );
    let unknown = d.patch(&d.manager, &path, json!({"total": 0})).await;
    assert_eq!(
        unknown.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        unknown.body
    );
    assert_eq!(
        d.patch(&d.tech, &path, json!({"note": "x"})).await.status,
        StatusCode::FORBIDDEN
    );
    let app = app_pool(&d.h.owner).await?;
    let input = sales::dto::SaleUpdate {
        note: Some("x".into()),
    };
    for actor in [
        d.manager_without(P::CustomerHealthNotesWrite),
        d.manager_without(P::PackageSell),
        d.former(),
    ] {
        let r = sales::service::update_note(
            &app,
            &d.h.cipher,
            &actor,
            Uuid::parse_str(&sale)?,
            Some(&input),
        )
        .await;
        assert!(forbidden(&r), "{r:?}");
    }

    // A day before today: the manager is locked out (before any body check, F17); the owner is not.
    sqlx::query("UPDATE app.sale SET business_date = business_date - 1 WHERE id = $1")
        .bind(Uuid::parse_str(&sale)?)
        .execute(&d.h.owner)
        .await?;
    for body in [json!({"note": "x"}), json!({"note": "x".repeat(2001)})] {
        let locked = d.patch(&d.manager, &path, body).await;
        assert_eq!(locked.status, StatusCode::FORBIDDEN, "{}", locked.body);
        assert_eq!(locked.body["code"], "day.locked");
    }
    let owner = d.patch(&d.owner, &path, json!({"note": NOTE_A})).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);

    // Only an open package sale: not a visit's one-off sale, not a voided sale, not another spa's.
    let visit = d
        .post(
            &d.manager,
            "/visits",
            json!({"customer_id": d.ha,
                   "lines": [{"service_id": d.shampoo, "source": "one_off", "staff_id": d.mai}],
                   "payments": [{"method": "cash", "amount": 49_000}]}),
        )
        .await;
    assert_eq!(visit.status, StatusCode::CREATED, "{}", visit.body);
    let one_off = visit.body["sale_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let r = d
        .patch(
            &d.manager,
            &format!("/sales/{one_off}"),
            json!({"note": "x"}),
        )
        .await;
    assert_eq!(
        (r.status, r.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("sale.not_package")),
        "{}",
        r.body
    );
    let other = d.post(&d.manager, "/sales", d.sale(None)).await;
    let other_id = id_of(&other);
    let void = d
        .post(
            &d.manager,
            &format!("/sales/{other_id}/void"),
            json!({"reason": "Khách đổi ý"}),
        )
        .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);
    let r = d
        .patch(
            &d.manager,
            &format!("/sales/{other_id}"),
            json!({"note": "x"}),
        )
        .await;
    assert_eq!(
        (r.status, r.body["code"].as_str()),
        (StatusCode::CONFLICT, Some("sale.voided"))
    );
    identity::provisioning::create_tenant(&d.h.owner, "spa-b", "Spa B", "Cơ sở 1").await?;
    add_member(
        &d.h.owner,
        "spa-b",
        "0987000999",
        "Chủ Spa B",
        SystemRole::Owner,
    )
    .await?;
    let stranger = session_for(&d.h, "0987000999").await;
    let r = d.patch(&stranger, &path, json!({"note": "x"})).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.body);
    Ok(())
}
