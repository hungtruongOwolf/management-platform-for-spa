//! "Nhật ký thao tác" (`GET /audit-log`, ADR-0021, spec management-views §11): the owner alone,
//! tenant isolation, one representative entry per group, the person / group / range filters,
//! keyset pagination that never skips or repeats, legacy actions, and no note text ever.

mod common;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Asia::Ho_Chi_Minh;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    post_customer, session_for,
};
use identity::SystemRole;
use identity::provisioning::create_tenant;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";
const TECH_PHONE: &str = "0912000222";
const OTHER_OWNER_PHONE: &str = "0913000333";
const PRICE: i64 = 150_000;
const DISCOUNT: i64 = 10_000;
/// The price after the owner raises it (the visit is recorded after).
const NEW_PRICE: i64 = PRICE + 20_000;
const VISIT_NOTE: &str = "Đau vai gáy, ấn nhẹ";
const HEALTH_NOTE: &str = "Huyết áp cao, tránh xông nóng";
const ATTENDANCE_NOTE: &str = "Con ốm, nghỉ đột xuất";
const VOID_REASON: &str = "Khách đổi ý";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

fn today() -> NaiveDate {
    Utc::now().with_timezone(&Ho_Chi_Minh).date_naive()
}

fn id(reply: &Reply) -> Uuid {
    reply.body["id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_default()
}

async fn post(h: &Harness, session: &str, path: &str, body: Value) -> Reply {
    let k = Uuid::now_v7().to_string();
    call_with_headers(
        &h.app,
        "POST",
        path,
        Some(session),
        Some(body),
        &[("Idempotency-Key", &k)],
    )
    .await
}

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    tech: String,
    tenant: Uuid,
    manager_user: Uuid,
    lan: Uuid,
    customer: Uuid,
    service: Uuid,
}

async fn user_id(h: &Harness, e164: &str) -> Setup<Uuid> {
    Ok(
        sqlx::query_scalar("SELECT id FROM app.user_account WHERE phone_e164 = $1")
            .bind(e164)
            .fetch_one(&h.owner)
            .await?,
    )
}

/// One spa where the owner and the manager did a bit of everything today, through the API.
async fn spa(pool: PgPool) -> Setup<Spa> {
    let h = harness(pool).await?;
    add_member(
        &h.owner,
        "vpyd",
        MANAGER_PHONE,
        "Quản lý Hà",
        SystemRole::Manager,
    )
    .await?;
    add_member(
        &h.owner,
        "vpyd",
        TECH_PHONE,
        "KTV Bình",
        SystemRole::Technician,
    )
    .await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let tech = session_for(&h, TECH_PHONE).await;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let manager_user = user_id(&h, "+84912000111").await?;
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;

    let category = call(
        &h.app,
        "POST",
        "/service-categories",
        Some(&owner),
        Some(json!({"name": "Dưỡng sinh"})),
    )
    .await;
    let created = call(
        &h.app,
        "POST",
        "/services",
        Some(&owner),
        Some(json!({
            "category_id": id(&category), "name": "Gội đầu dưỡng sinh", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": PRICE}
        })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
    let service = id(&created);
    // Settings: the price goes up.
    let repriced = call_with_headers(
        &h.app,
        "PUT",
        &format!("/services/{service}"),
        Some(&owner),
        Some(json!({
            "category_id": id(&category), "name": "Gội đầu dưỡng sinh", "description": null,
            "duration_min": 60, "recommended": false, "active": true, "prices": {"all": NEW_PRICE}
        })),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(repriced.status, StatusCode::OK, "{}", repriced.body);

    // Customers: the manager creates one, then corrects the name and the phone.
    let customer = post_customer(&h.app, Some(&manager), Some(json!({
        "full_name": "Lê Mai", "phone": "0905111222", "gender": "female", "health_note": HEALTH_NOTE
    }))).await;
    assert_eq!(customer.status, StatusCode::CREATED, "{}", customer.body);
    let customer = id(&customer);
    let renamed = call_with_headers(
        &h.app,
        "PUT",
        &format!("/customers/{customer}"),
        Some(&manager),
        Some(json!({
            "full_name": "Lê Thị Mai", "phone": "0912333444", "gender": "female"
        })),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);

    // Front desk: the manager records a visit with a discount and a session note, then voids it.
    let visit = post(
        &h,
        &manager,
        "/visits",
        json!({
            "customer_id": customer, "note": VISIT_NOTE,
            "lines": [{"service_id": service, "source": "one_off", "staff_id": lan}],
            "discount": {"amount": DISCOUNT, "reason": "Khách quen"},
            "payments": [{"method": "cash", "amount": NEW_PRICE - DISCOUNT}]
        }),
    )
    .await;
    assert_eq!(visit.status, StatusCode::CREATED, "{}", visit.body);
    let void = post(
        &h,
        &manager,
        &format!("/visits/{}/void", id(&visit)),
        json!({"reason": VOID_REASON}),
    )
    .await;
    assert_eq!(void.status, StatusCode::OK, "{}", void.body);

    // Staff: the owner records Lan's day off, with a note that must never show.
    let attended = call(
        &h.app,
        "PUT",
        "/attendance",
        Some(&owner),
        Some(json!({"entries": [
            {"staff_id": lan, "date": today(), "status": "leave_dx", "note": ATTENDANCE_NOTE}
        ]})),
    )
    .await;
    assert_eq!(attended.status, StatusCode::OK, "{}", attended.body);
    // Settings: booking limit.
    let settings = call(
        &h.app,
        "PUT",
        "/appointment-settings",
        Some(&owner),
        Some(json!({"max_per_slot": 4})),
    )
    .await;
    assert_eq!(settings.status, StatusCode::OK, "{}", settings.body);
    Ok(Spa {
        h,
        owner,
        manager,
        tech,
        tenant,
        manager_user,
        lan,
        customer,
        service,
    })
}

async fn log(s: &Spa, session: &str, query: &str) -> Reply {
    call(
        &s.h.app,
        "GET",
        &format!("/audit-log{query}"),
        Some(session),
        None,
    )
    .await
}

fn items(r: &Reply) -> Vec<Value> {
    r.body["items"].as_array().cloned().unwrap_or_default()
}

fn find<'a>(list: &'a [Value], action: &str) -> &'a Value {
    list.iter()
        .find(|e| e["action"] == action)
        .unwrap_or_else(|| panic!("no {action} in {list:?}"))
}

/// Plants one audit row (legacy keys, other actors, past instants), as the schema owner.
#[allow(clippy::too_many_arguments)]
async fn plant(
    owner: &PgPool,
    tenant: Uuid,
    kind: &str,
    actor: Option<Uuid>,
    action: &str,
    entity_type: &str,
    entity: Uuid,
    at: DateTime<Utc>,
) -> Setup<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, at, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'Kiểm tra định kỳ')",
    )
    .bind(id)
    .bind(tenant)
    .bind(actor)
    .bind(kind)
    .bind(action)
    .bind(entity_type)
    .bind(entity)
    .bind(at)
    .execute(owner)
    .await?;
    Ok(id)
}

/// Plants a row of `actor`'s with an `after` (the shape the writers store).
async fn plant_after(
    s: &Spa,
    action: &str,
    entity_type: &str,
    entity: Uuid,
    at: DateTime<Utc>,
    after: Value,
) -> Setup<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO app.audit_log (id, tenant_id, actor_user_id, actor_kind, action, entity_type, entity_id, at, after)
         VALUES ($1, $2, $3, 'user', $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(s.tenant)
    .bind(s.manager_user)
    .bind(action)
    .bind(entity_type)
    .bind(entity)
    .bind(at)
    .bind(after)
    .execute(&s.h.owner)
    .await?;
    Ok(id)
}

fn actions_of(list: &[Value]) -> Vec<String> {
    list.iter()
        .filter_map(|e| e["action"].as_str().map(str::to_owned))
        .collect()
}

/// The rows the trail holds for an action, whatever the screen shows.
async fn stored(s: &Spa, action: &str) -> Setup<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM app.audit_log WHERE tenant_id = $1 AND action = $2",
    )
    .bind(s.tenant)
    .bind(action)
    .fetch_one(&s.h.owner)
    .await?)
}

/// A package plan of the fixture's service, for the checkout and package-sale tests.
async fn plan(s: &Spa) -> Setup<Uuid> {
    let plan = call(
        &s.h.app,
        "POST",
        "/package-plans",
        Some(&s.owner),
        Some(json!({
            "name": PLAN_NAME, "price": PLAN_PRICE, "validity_months": 6, "gift_note": null,
            "active": true, "items": [{"service_id": s.service, "sessions": 10, "bonus_sessions": 0}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    Ok(id(&plan))
}

const PLAN_NAME: &str = "Gội đầu 10 buổi";
const PLAN_PRICE: i64 = 1_500_000;

/// EM 2026-11-11 ("một dòng thôi vì khi ghi là thu tiền luôn rồi"): recording a paid visit is one
/// line that carries its money and reason, voiding it is one line with the reversal; the
/// `sale.create` / `sale.void` rows stay in the trail.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_visit_and_its_sale_read_as_one_line(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    for kind in ["", "&kind=front_desk"] {
        let page = log(&s, &s.owner, &format!("?limit=100{kind}")).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        let list = items(&page);
        let actions = actions_of(&list);
        assert!(
            !actions.iter().any(|a| a.starts_with("sale.")),
            "a visit's sale shown on its own: {actions:?}"
        );
        let visit = find(&list, "visit.create");
        assert_eq!(visit["folded"], json!(["sale.create"]));
        assert_eq!(
            (
                visit["amount"].clone(),
                visit["discount"].clone(),
                visit["reason"].clone()
            ),
            (
                json!(NEW_PRICE - DISCOUNT),
                json!(DISCOUNT),
                json!("Khách quen")
            )
        );
        assert_eq!(visit["packages"], json!([]));
        let void = find(&list, "visit.void");
        assert_eq!(void["folded"], json!(["sale.void"]));
        assert_eq!(void["amount"], json!(NEW_PRICE - DISCOUNT));
        assert_eq!(void["reason"], VOID_REASON);
        assert_eq!(
            actions.iter().filter(|a| a.starts_with("visit.")).count(),
            2
        );
    }
    // Traceability: both rows of each action are still stored.
    assert_eq!(stored(&s, "sale.create").await?, 1);
    assert_eq!(stored(&s, "sale.void").await?, 1);
    Ok(())
}

/// A package sold in a visit's checkout folds into the visit's line (named, its price in the
/// money collected, found by the "Gói liệu trình" group too). A package sale on its own and its
/// void are one line each; a package cancelled with a refund stays its own line.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn packages_fold_into_the_action_that_sold_or_cancelled_them(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let plan = plan(&s).await?;
    let checkout = post(
        &s.h,
        &s.manager,
        "/visits",
        json!({
            "customer_id": s.customer,
            "lines": [
                {"service_id": s.service, "source": "package", "package_ref": "p1", "staff_id": s.lan},
                {"service_id": s.service, "source": "one_off", "staff_id": s.lan},
            ],
            "package_sales": [{"ref": "p1", "package_plan_id": plan, "closer_staff_id": s.lan}],
            "payments": [{"method": "cash", "amount": PLAN_PRICE + NEW_PRICE}],
        }),
    )
    .await;
    assert_eq!(checkout.status, StatusCode::CREATED, "{}", checkout.body);
    let in_visit = checkout.body["package_sales"][0]["customer_package_ids"][0].clone();
    let sold = post(
        &s.h,
        &s.manager,
        "/sales",
        json!({
            "customer_id": s.customer, "closer_staff_id": s.lan,
            "lines": [{"package_plan_id": plan}],
            "payments": [{"method": "bank_transfer", "amount": PLAN_PRICE}],
        }),
    )
    .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    let voided = post(
        &s.h,
        &s.manager,
        &format!("/sales/{}/void", id(&sold)),
        json!({"reason": "Bán nhầm gói"}),
    )
    .await;
    assert_eq!(voided.status, StatusCode::OK, "{}", voided.body);
    let package = in_visit.as_str().map(str::to_owned).unwrap_or_default();
    let refunded = post(
        &s.h,
        &s.owner,
        &format!("/customer-packages/{package}/cancel"),
        json!({"reason": "Khách chuyển nhà", "refund_amount": 300_000, "refund_method": "cash"}),
    )
    .await;
    assert_eq!(refunded.status, StatusCode::OK, "{}", refunded.body);

    let page = log(&s, &s.owner, "?limit=100").await;
    let list = items(&page);
    let actions = actions_of(&list);
    let count = |a: &str| actions.iter().filter(|x| *x == a).count();
    assert_eq!(count("customer_package.create"), 0, "{actions:?}");
    assert_eq!(count("visit.create"), 2, "{actions:?}");
    assert_eq!((count("sale.create"), count("sale.void")), (1, 1));
    assert_eq!(count("customer_package.cancel"), 1, "{actions:?}");

    // The checkout: one line, every sale's money, the package named.
    let visit = list
        .iter()
        .find(|e| e["action"] == "visit.create" && e["packages"] == json!([PLAN_NAME]))
        .unwrap_or_else(|| panic!("no checkout line in {list:?}"));
    assert_eq!(visit["amount"], json!(PLAN_PRICE + NEW_PRICE));
    let mut folded: Vec<String> = visit["folded"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    folded.sort();
    assert_eq!(
        folded,
        ["customer_package.create", "sale.create", "sale.create"]
    );
    // The package sale on its own: one line naming the package; its void likewise.
    let sale = find(&list, "sale.create");
    assert_eq!(
        (
            sale["packages"].clone(),
            sale["folded"].clone(),
            sale["amount"].clone()
        ),
        (
            json!([PLAN_NAME]),
            json!(["customer_package.create"]),
            json!(PLAN_PRICE)
        )
    );
    let void = find(&list, "sale.void");
    assert_eq!(
        (
            void["packages"].clone(),
            void["folded"].clone(),
            void["amount"].clone(),
            void["reason"].clone()
        ),
        (
            json!([PLAN_NAME]),
            json!(["customer_package.cancel"]),
            json!(PLAN_PRICE),
            json!("Bán nhầm gói")
        )
    );
    // A cancellation with a refund is its own action.
    let cancel = find(&list, "customer_package.cancel");
    assert_eq!(
        (cancel["amount"].clone(), cancel["folded"].clone()),
        (json!(300_000), json!([]))
    );

    // "Gói liệu trình" finds every line that sold or cancelled a package.
    let packages = log(&s, &s.owner, "?limit=100&kind=packages").await;
    let mut found = actions_of(&items(&packages));
    found.sort();
    assert_eq!(
        found,
        [
            "customer_package.cancel",
            "sale.create",
            "sale.void",
            "visit.create"
        ]
    );
    assert_eq!(stored(&s, "customer_package.create").await?, 2);
    assert_eq!(stored(&s, "customer_package.cancel").await?, 2);
    Ok(())
}

/// "Sửa lượt" and "chuyển sang gói" are one line each: the void of the old visit, the new visit
/// and their sales fold into the correction, which carries the difference and the reason.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_correction_or_conversion_reads_as_one_line(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let plan = plan(&s).await?;
    let one_off = |customer: Uuid| {
        json!({
            "customer_id": customer,
            "lines": [{"service_id": s.service, "source": "one_off", "staff_id": s.lan}],
            "payments": [{"method": "cash", "amount": NEW_PRICE}],
        })
    };
    let first = post(&s.h, &s.manager, "/visits", one_off(s.customer)).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    let corrected = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/visits/{}/correct", id(&first)),
        Some(&s.manager),
        Some(json!({
            "reason": "thêm giảm giá", "customer_id": s.customer,
            "lines": [{"service_id": s.service, "source": "one_off", "staff_id": s.lan}],
            "discount": {"amount": DISCOUNT, "reason": "Khách quen"},
            "payments": [{"method": "cash", "amount": NEW_PRICE - DISCOUNT}],
        })),
        &[
            ("Idempotency-Key", &Uuid::now_v7().to_string()),
            ("If-Match", "\"1\""),
        ],
    )
    .await;
    assert_eq!(corrected.status, StatusCode::CREATED, "{}", corrected.body);
    let second = post(&s.h, &s.manager, "/visits", one_off(s.customer)).await;
    assert_eq!(second.status, StatusCode::CREATED, "{}", second.body);
    let converted = call_with_headers(
        &s.h.app,
        "POST",
        &format!("/visits/{}/convert-to-package", id(&second)),
        Some(&s.owner),
        Some(json!({
            "package_sale": {"package_plan_id": plan, "closer_staff_id": s.lan},
            "payments": [{"method": "bank_transfer", "amount": PLAN_PRICE - NEW_PRICE}],
        })),
        &[
            ("Idempotency-Key", &Uuid::now_v7().to_string()),
            ("If-Match", "\"1\""),
        ],
    )
    .await;
    assert_eq!(converted.status, StatusCode::CREATED, "{}", converted.body);

    let page = log(&s, &s.owner, "?limit=100&kind=front_desk").await;
    let list = items(&page);
    let sorted = |v: &Value| -> Vec<String> {
        let mut keys: Vec<String> = v
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| k.as_str().map(str::to_owned))
            .collect();
        keys.sort();
        keys
    };
    let correct = find(&list, "visit.correct");
    assert_eq!(
        sorted(&correct["folded"]),
        ["sale.create", "sale.void", "visit.create", "visit.void"]
    );
    assert_eq!(correct["amount"], json!(-DISCOUNT));
    assert_eq!(correct["reason"], "Sửa lượt: thêm giảm giá");
    let convert = find(&list, "visit.convert_to_package");
    assert_eq!(
        sorted(&convert["folded"]),
        [
            "customer_package.create",
            "sale.create",
            "sale.void",
            "visit.correct",
            "visit.create",
            "visit.void"
        ]
    );
    assert_eq!(convert["packages"], json!([PLAN_NAME]));
    // Today's lines: the fixture's visit + void, the two visits recorded, one correction, one
    // conversion — nothing else from the desk.
    let mut actions = actions_of(&list);
    actions.sort();
    assert_eq!(
        actions,
        [
            "visit.convert_to_package",
            "visit.correct",
            "visit.create",
            "visit.create",
            "visit.create",
            "visit.void"
        ]
    );
    // The package it sold makes the conversion a "Gói liệu trình" line as well.
    let packages = actions_of(&items(&log(&s, &s.owner, "?limit=100&kind=packages").await));
    assert_eq!(packages, ["visit.convert_to_package"]);
    Ok(())
}

/// Many pairs written at one instant by one person page through without ever splitting a pair
/// or showing a sale on its own; a sale of another instant or another person is not folded.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn pages_never_split_an_action(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let instant = Utc::now() - Duration::hours(2);
    let mut visits = BTreeSet::new();
    for i in 0..9 {
        let (visit, sale) = (Uuid::now_v7(), Uuid::now_v7());
        let sale_row = || {
            plant_after(
                &s,
                "sale.create",
                "sale",
                sale,
                instant,
                json!({"visit_id": visit, "total": 100_000, "discount_amount": 0}),
            )
        };
        // Either write order: the sale's row sorts before or after its visit's.
        if i % 2 == 0 {
            sale_row().await?;
        }
        visits.insert(
            plant_after(
                &s,
                "visit.create",
                "visit",
                visit,
                instant,
                json!({"sale_id": sale, "total": 100_000, "package_sale_ids": []}),
            )
            .await?,
        );
        if i % 2 == 1 {
            sale_row().await?;
        }
    }
    // Not the same action: a second earlier, or the same instant written by someone else.
    let (visit, sale) = (Uuid::now_v7(), Uuid::now_v7());
    plant_after(
        &s,
        "visit.create",
        "visit",
        visit,
        instant,
        json!({"sale_id": sale, "total": 1}),
    )
    .await?;
    let alone = plant_after(
        &s,
        "sale.create",
        "sale",
        sale,
        instant - Duration::seconds(1),
        json!({"visit_id": visit, "total": 1}),
    )
    .await?;
    let other_person = plant(
        &s.h.owner,
        s.tenant,
        "system",
        None,
        "sale.create",
        "sale",
        sale,
        instant,
    )
    .await?;

    let all = log(&s, &s.owner, "?limit=100").await;
    let expected: Vec<Value> = items(&all).iter().map(|e| e["id"].clone()).collect();
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let query = cursor
            .as_ref()
            .map_or("?limit=4".to_owned(), |c| format!("?limit=4&cursor={c}"));
        let page = log(&s, &s.owner, &query).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        let list = items(&page);
        assert!(list.len() <= 4);
        for e in &list {
            let id = e["id"].as_str().and_then(|i| Uuid::parse_str(i).ok());
            if id.is_some_and(|i| visits.contains(&i)) {
                assert_eq!(e["folded"], json!(["sale.create"]), "{e}");
                assert_eq!(e["amount"], json!(100_000));
            }
        }
        seen.extend(list.iter().map(|e| e["id"].clone()));
        match page.body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    assert_eq!(seen, expected, "same lines, same order, none twice");
    let shown: BTreeSet<Uuid> = seen
        .iter()
        .filter_map(|v| v.as_str().and_then(|i| Uuid::parse_str(i).ok()))
        .collect();
    assert!(visits.is_subset(&shown));
    assert!(shown.contains(&alone) && shown.contains(&other_person));
    // The nine planted sales folded, the two that are not part of a visit's action did not.
    let sales = items(&all)
        .iter()
        .filter(|e| e["action"] == "sale.create")
        .count();
    assert_eq!(sales, 2);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn only_the_owner_reads_the_activity_log(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let owner = log(&s, &s.owner, "").await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(
        owner
            .headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    for session in [&s.manager, &s.tech] {
        let refused = log(&s, session, "").await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body);
        assert!(refused.body.get("items").is_none());
    }
    assert_eq!(
        call(&s.h.app, "GET", "/audit-log", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    // A bad query is refused only after the right (F17): the manager still gets 403.
    assert_eq!(
        log(&s, &s.manager, "?actor=x").await.status,
        StatusCode::FORBIDDEN
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn another_spas_entries_never_appear(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
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
    let theirs = post_customer(
        &s.h.app,
        Some(&other),
        Some(json!({"full_name": "Khách Bên Kia", "phone": "0905999888"})),
    )
    .await;
    assert_eq!(theirs.status, StatusCode::CREATED, "{}", theirs.body);

    let mine = log(&s, &s.owner, "?limit=100").await;
    let text = mine.body.to_string();
    assert!(
        !text.contains("Khách Bên Kia") && !text.contains("Chủ Khác"),
        "{text}"
    );
    let other_page = call(&s.h.app, "GET", "/audit-log?limit=100", Some(&other), None).await;
    assert_eq!(other_page.status, StatusCode::OK, "{}", other_page.body);
    let actions: BTreeSet<String> = items(&other_page)
        .iter()
        .filter_map(|e| e["action"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(actions, BTreeSet::from(["customer.create".to_owned()]));
    let text = other_page.body.to_string();
    assert!(
        !text.contains("Lê Thị Mai") && !text.contains("Quản lý Hà"),
        "{text}"
    );
    // A planted cross-spa row naming this spa's customer resolves nothing for the other owner.
    let planted_tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'khac'")
        .fetch_one(&s.h.owner)
        .await?;
    plant(
        &s.h.owner,
        planted_tenant,
        "system",
        None,
        "customer.update",
        "customer",
        s.customer,
        Utc::now(),
    )
    .await?;
    let other_page = call(&s.h.app, "GET", "/audit-log?limit=100", Some(&other), None).await;
    assert!(
        !other_page.body.to_string().contains("Lê Thị Mai"),
        "{}",
        other_page.body
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn each_group_reads_as_a_structured_sentence(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let owner_user = user_id(&s.h, "+84905123456").await?;
    // The platform team and the login screen, as their writers store them.
    plant(
        &s.h.owner,
        s.tenant,
        "platform_admin",
        Some(Uuid::now_v7()),
        "account.locked_by_platform",
        "user",
        s.manager_user,
        Utc::now(),
    )
    .await?;
    let page = log(&s, &s.owner, "?limit=100").await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body);
    let list = items(&page);

    // Never a note's text: the session note, the health note, the attendance note.
    let text = page.body.to_string();
    for secret in [VISIT_NOTE, HEALTH_NOTE, ATTENDANCE_NOTE, "redacted"] {
        assert!(!text.contains(secret), "{secret:?} leaked: {text}");
    }

    // Quầy: "Hà hủy lượt của chị Mai · lý do: khách đổi ý".
    let void = find(&list, "visit.void");
    assert_eq!(void["kind"], "front_desk");
    assert_eq!(
        void["actor"],
        json!({"kind": "user", "user_id": s.manager_user, "name": "Quản lý Hà"})
    );
    assert_eq!(void["subject"]["kind"], "customer");
    assert_eq!(void["subject"]["id"], json!(s.customer));
    assert_eq!(void["subject"]["name"], "Lê Thị Mai");
    assert_eq!(void["subject"]["gender"], "female");
    assert_eq!(void["subject"]["on"], json!(today()));
    assert_eq!(void["reason"], VOID_REASON);
    let visit = find(&list, "visit.create");
    assert_eq!(
        (visit["amount"].clone(), visit["discount"].clone()),
        (json!(NEW_PRICE - DISCOUNT), json!(DISCOUNT))
    );
    assert_eq!(visit["reason"], "Khách quen");

    // Khách hàng: old → new of the name and the phone, the other fields named only.
    let update = find(&list, "customer.update");
    assert_eq!(update["kind"], "customers");
    assert_eq!(
        update["changes"],
        json!([
            {"field": "full_name", "format": "text", "old": "Lê Mai", "new": "Lê Thị Mai"},
            {"field": "phone", "format": "phone", "old": "+84905111222", "new": "+84912333444"},
        ])
    );
    assert!(update.get("reason").is_none());
    let create = find(&list, "customer.create");
    assert_eq!(create["changes"], json!([]));

    // Nhân sự: who, about whom, which day, what status (not the note).
    let attendance = find(&list, "attendance.record");
    assert_eq!(attendance["kind"], "staff");
    assert_eq!(attendance["actor"]["user_id"], json!(owner_user));
    assert_eq!(
        (
            attendance["subject"]["kind"].clone(),
            attendance["subject"]["name"].clone(),
            attendance["subject"]["id"].clone()
        ),
        (json!("staff"), json!("KTV Lan"), json!(s.lan))
    );
    assert_eq!(attendance["subject"]["on"], json!(today()));
    assert_eq!(
        attendance["changes"],
        json!([
            {"field": "status", "format": "code", "new": "leave_dx"},
            {"field": "has_note", "format": "flag", "new": "true"},
        ])
    );

    // Danh mục & cài đặt: the price, old → new; the booking limit.
    let price = find(&list, "catalog.service_update");
    assert_eq!(price["kind"], "settings");
    assert_eq!(
        price["subject"],
        json!({"kind": "service", "name": "Gội đầu dưỡng sinh"})
    );
    assert_eq!(
        price["changes"],
        json!([{"field": "price", "format": "money", "old": PRICE.to_string(), "new": NEW_PRICE.to_string()}])
    );
    let settings = find(&list, "appointment.settings");
    assert_eq!(
        settings["changes"],
        json!([{"field": "max_per_slot", "format": "number", "new": "4"}])
    );
    assert_eq!(settings["subject"], json!({"kind": "spa"}));

    // Tài khoản & bảo mật: the platform team is never named, and no operator id leaves.
    let locked = find(&list, "account.locked_by_platform");
    assert_eq!(locked["kind"], "accounts");
    assert_eq!(locked["actor"], json!({"kind": "platform"}));
    assert_eq!(
        locked["subject"],
        json!({"kind": "account", "name": "Quản lý Hà"})
    );

    // The person filter lists the spa's logins.
    let people: BTreeSet<String> = page.body["people"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        people,
        BTreeSet::from([
            "Chủ Spa".to_owned(),
            "KTV Bình".to_owned(),
            "Quản lý Hà".to_owned()
        ])
    );
    let _ = s.service;
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn filters_by_person_group_and_range(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let forgot = call(
        &s.h.app,
        "POST",
        "/auth/password-reset-requests",
        None,
        Some(json!({"phone": TECH_PHONE})),
    )
    .await;
    assert_eq!(forgot.status, StatusCode::ACCEPTED, "{}", forgot.body);
    let ten_days_ago = Utc::now() - Duration::days(10);
    plant(
        &s.h.owner,
        s.tenant,
        "user",
        Some(s.manager_user),
        "customer.follow_up",
        "customer",
        s.customer,
        ten_days_ago,
    )
    .await?;

    let actions = |r: &Reply| -> BTreeSet<String> {
        items(r)
            .iter()
            .filter_map(|e| e["action"].as_str().map(str::to_owned))
            .collect()
    };
    let by_manager = log(
        &s,
        &s.owner,
        &format!("?limit=100&actor={}", s.manager_user),
    )
    .await;
    assert_eq!(by_manager.status, StatusCode::OK, "{}", by_manager.body);
    assert!(
        items(&by_manager)
            .iter()
            .all(|e| e["actor"]["user_id"] == json!(s.manager_user))
    );
    let mine = actions(&by_manager);
    for action in [
        "customer.create",
        "customer.update",
        "visit.create",
        "visit.void",
    ] {
        assert!(mine.contains(action), "{action} in {mine:?}");
    }
    assert!(!mine.contains("attendance.record") && !mine.contains("catalog.service_update"));
    let system = log(&s, &s.owner, "?actor=system").await;
    assert_eq!(
        actions(&system),
        BTreeSet::from(["account.password_reset_requested".to_owned()])
    );
    assert_eq!(items(&system)[0]["actor"], json!({"kind": "system"}));
    assert_eq!(items(&log(&s, &s.owner, "?actor=platform").await).len(), 0);

    let customers = log(&s, &s.owner, "?limit=100&kind=customers").await;
    assert_eq!(
        actions(&customers),
        BTreeSet::from(["customer.create", "customer.update"].map(str::to_owned))
    );
    let staff = log(&s, &s.owner, "?kind=staff").await;
    assert_eq!(
        actions(&staff),
        BTreeSet::from(["attendance.record".to_owned()])
    );

    // The default range is the last 7 days; a wider one reaches the planted entry.
    let default = log(&s, &s.owner, "?limit=100").await;
    assert_eq!(default.body["to"], json!(today()));
    assert_eq!(default.body["from"], json!(today() - Duration::days(6)));
    assert!(!actions(&default).contains("customer.follow_up"));
    let wide = log(
        &s,
        &s.owner,
        &format!("?limit=100&from={}", today() - Duration::days(30)),
    )
    .await;
    assert!(actions(&wide).contains("customer.follow_up"));
    let only_then = log(
        &s,
        &s.owner,
        &format!(
            "?from={0}&to={0}",
            (ten_days_ago.with_timezone(&Ho_Chi_Minh)).date_naive()
        ),
    )
    .await;
    assert_eq!(
        actions(&only_then),
        BTreeSet::from(["customer.follow_up".to_owned()])
    );

    for (query, field, code) in [
        (
            format!("?from={}&to={}", today(), today() - Duration::days(1)),
            "from",
            "after_to",
        ),
        (
            format!("?from={}", today() - Duration::days(366)),
            "to",
            "range_too_long",
        ),
        ("?actor=0905123456".to_owned(), "actor", "invalid"),
        ("?cursor=xyz".to_owned(), "cursor", "invalid"),
        ("?limit=101".to_owned(), "limit", "out_of_range"),
    ] {
        let r = log(&s, &s.owner, &query).await;
        assert_eq!(
            r.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query}: {}",
            r.body
        );
        assert_eq!(
            (
                r.body["errors"][0]["field"].as_str(),
                r.body["errors"][0]["code"].as_str()
            ),
            (Some(field), Some(code)),
            "{query}"
        );
    }
    // An unknown group is a malformed query (`400 query.invalid`, kernel::http::invalid_query).
    assert_eq!(
        log(&s, &s.owner, "?kind=nope").await.status,
        StatusCode::BAD_REQUEST
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn pages_never_skip_or_repeat_inside_one_instant(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    // Seven entries written by one transaction share an instant.
    let instant = Utc::now() - Duration::hours(1);
    for _ in 0..7 {
        plant(
            &s.h.owner,
            s.tenant,
            "user",
            Some(s.manager_user),
            "customer.follow_up",
            "customer",
            s.customer,
            instant,
        )
        .await?;
    }
    let all = log(&s, &s.owner, "?limit=100").await;
    let expected: Vec<Value> = items(&all).iter().map(|e| e["id"].clone()).collect();
    assert!(expected.len() >= 15, "{}", expected.len());
    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let query = cursor
            .as_ref()
            .map_or("?limit=3".to_owned(), |c| format!("?limit=3&cursor={c}"));
        let page = log(&s, &s.owner, &query).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        let list = items(&page);
        assert!(list.len() <= 3);
        seen.extend(list.iter().map(|e| e["id"].clone()));
        match page.body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => break,
        }
    }
    assert_eq!(seen, expected, "same entries, same order, none twice");
    // Newest first by (at, id): the planted instant's seven entries come out by id, descending.
    let tied: Vec<String> = items(&all)
        .iter()
        .filter(|e| e["action"] == "customer.follow_up")
        .filter_map(|e| e["id"].as_str().map(str::to_owned))
        .collect();
    let mut by_id = tied.clone();
    by_id.sort_by(|a, b| b.cmp(a));
    assert_eq!((tied.len(), &tied), (7, &by_id));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_legacy_action_reads_as_other_and_a_gone_subject_as_nameless(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    plant(
        &s.h.owner,
        s.tenant,
        "user",
        Some(s.manager_user),
        "visit.legacy_mark",
        "visit",
        Uuid::now_v7(),
        Utc::now(),
    )
    .await?;
    sqlx::query("UPDATE app.customer SET full_name = NULL, phone_e164 = NULL, anonymized_at = now() WHERE id = $1")
        .bind(s.customer)
        .execute(&s.h.owner)
        .await?;
    let other = log(&s, &s.owner, "?kind=other").await;
    assert_eq!(other.status, StatusCode::OK, "{}", other.body);
    let list = items(&other);
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["kind"], "other");
    assert_eq!(list[0]["action"], "visit.legacy_mark");
    // The visit row does not exist: the subject is a customer with no name and no link.
    assert_eq!(list[0]["subject"], json!({"kind": "customer"}));
    // An anonymised customer shows neither a name nor a link, on every entry about them.
    let page = log(&s, &s.owner, "?kind=front_desk&limit=100").await;
    for e in items(&page) {
        assert_eq!(e["subject"].get("name"), None, "{e}");
        assert_eq!(e["subject"].get("id"), None, "{e}");
    }
    // Nor the old / new name and phone its update kept (until anonymisation scrubs the trail).
    let customers = log(&s, &s.owner, "?kind=customers&limit=100").await;
    let text = customers.body.to_string();
    for value in ["Mai", "905111222", "912333444"] {
        assert!(
            !text.contains(value),
            "{value} shown for an anonymised customer: {text}"
        );
    }
    assert_eq!(
        find(&items(&customers), "customer.update")["changes"],
        json!([{"field": "full_name", "format": "text"}, {"field": "phone", "format": "phone"}])
    );
    Ok(())
}
