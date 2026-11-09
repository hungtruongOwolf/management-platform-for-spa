//! S8: voids and the package lifecycle (spec R1-1 US6: AC6.1–6.4, US7: AC7.1–7.4).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{
    Harness, OWNER_PHONE, Reply, TestResult, add_member, call, call_with_headers, harness,
    session_for,
};
use identity::SystemRole;
use kernel::testing::app_pool;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const MANAGER_PHONE: &str = "0912000111";

type Setup<T> = Result<T, Box<dyn std::error::Error>>;

struct Spa {
    h: Harness,
    owner: String,
    manager: String,
    /// Female customer.
    ha: String,
    lan: String,
    shampoo: String,
    plan: String,
}

struct Sold {
    sale: String,
    package: String,
    item: String,
}

fn str_of(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn code(reply: &Reply) -> (StatusCode, Option<&str>) {
    (reply.status, reply.body["code"].as_str())
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
    let owner = session_for(&h, OWNER_PHONE).await;
    let manager = session_for(&h, MANAGER_PHONE).await;
    let shampoo: Uuid =
        sqlx::query_scalar("SELECT id FROM app.service WHERE name = 'Cơ Bản – Dầu thảo dược'")
            .fetch_one(&h.owner)
            .await?;
    let plan = call(
        &h.app,
        "POST",
        "/package-plans",
        Some(&owner),
        Some(json!({
            "name": "Gội đầu dưỡng sinh 10 buổi + tặng 2", "price": 1_500_000, "validity_months": 6,
            "gift_note": null, "active": true,
            "items": [{"service_id": shampoo, "sessions": 10, "bonus_sessions": 2}],
        })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::CREATED, "{}", plan.body);
    let ha = call(
        &h.app,
        "POST",
        "/customers",
        Some(&manager),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111222", "gender": "female"})),
    )
    .await;
    assert_eq!(ha.status, StatusCode::CREATED, "{}", ha.body);
    let lan = org::provisioning::add_staff(&h.owner, "vpyd", "KTV Lan", "Kỹ thuật viên").await?;
    Ok(Spa {
        owner,
        manager,
        ha: str_of(&ha.body["id"]),
        lan: lan.to_string(),
        shampoo: shampoo.to_string(),
        plan: str_of(&plan.body["id"]),
        h,
    })
}

impl Spa {
    async fn post(&self, who: &str, path: &str, body: Value) -> Reply {
        let key = Uuid::now_v7().to_string();
        self.post_with(who, path, body, &[("idempotency-key", &key)])
            .await
    }

    async fn post_with(
        &self,
        who: &str,
        path: &str,
        body: Value,
        headers: &[(&str, &str)],
    ) -> Reply {
        call_with_headers(&self.h.app, "POST", path, Some(who), Some(body), headers).await
    }

    async fn get(&self, path: &str) -> Reply {
        call(&self.h.app, "GET", path, Some(&self.owner), None).await
    }

    /// The 12-session shampoo package, 1.500.000đ cash.
    async fn sell(&self) -> Setup<Sold> {
        let sold = self
            .post(
                &self.manager,
                "/sales",
                json!({
                    "customer_id": self.ha, "closer_staff_id": self.lan,
                    "lines": [{"package_plan_id": self.plan}],
                    "payments": [{"method": "cash", "amount": 1_500_000}],
                }),
            )
            .await;
        assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
        let p = &sold.body["customer_packages"][0];
        Ok(Sold {
            sale: str_of(&sold.body["id"]),
            package: str_of(&p["id"]),
            item: str_of(&p["items"][0]["id"]),
        })
    }

    /// One session of `item`, plus (optionally) a one-off shampoo paid 49.000đ in cash.
    async fn visit(&self, item: &str, with_one_off: bool) -> Reply {
        let mut lines = vec![json!({"service_id": self.shampoo, "source": "package",
                                    "customer_package_item_id": item, "staff_id": self.lan})];
        let mut payments = vec![];
        if with_one_off {
            lines.push(
                json!({"service_id": self.shampoo, "source": "one_off", "staff_id": self.lan}),
            );
            payments.push(json!({"method": "cash", "amount": 49_000}));
        }
        let v = self
            .post(
                &self.manager,
                "/visits",
                json!({"customer_id": self.ha, "lines": lines, "payments": payments}),
            )
            .await;
        assert_eq!(v.status, StatusCode::CREATED, "{}", v.body);
        v
    }

    async fn void_visit(&self, who: &str, visit: &str, reason: Value) -> Reply {
        self.post(
            who,
            &format!("/visits/{visit}/void"),
            json!({"reason": reason}),
        )
        .await
    }

    async fn void_sale(&self, who: &str, sale: &str, reason: Value) -> Reply {
        self.post(
            who,
            &format!("/sales/{sale}/void"),
            json!({"reason": reason}),
        )
        .await
    }

    async fn remaining(&self, item: &str) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT sessions_remaining FROM app.customer_package_item WHERE id = $1::uuid",
        )
        .bind(item)
        .fetch_one(&self.h.owner)
        .await
    }

    async fn count(&self, sql: &str) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
            .fetch_one(&self.h.owner)
            .await
    }

    async fn exec(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()))
            .execute(&self.h.owner)
            .await
            .map(|_| ())
    }

    /// Every session's password confirmation is older than the 5-minute re-auth window, so a
    /// passing owner action proves it does not ask for the password (EM decision 2026-11-05).
    async fn age_reauth(&self) -> Result<(), sqlx::Error> {
        self.exec("UPDATE app.session SET recent_auth_at = now() - interval '10 minutes'")
            .await
    }

    async fn extend(&self, who: &str, package: &str, version: &str, body: Value) -> Reply {
        let key = Uuid::now_v7().to_string();
        let if_match = format!("\"{version}\"");
        self.post_with(
            who,
            &format!("/customer-packages/{package}/extend"),
            body,
            &[("idempotency-key", &key), ("If-Match", &if_match)],
        )
        .await
    }

    async fn cancel(&self, who: &str, package: &str, body: Value) -> Reply {
        self.post(who, &format!("/customer-packages/{package}/cancel"), body)
            .await
    }
}

// ------------------------------------------------------------------------------- visits (US6)

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn voiding_a_visit_gives_sessions_back_and_reverses_its_payment(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let v = s.visit(&sold.item, true).await;
    let visit = str_of(&v.body["id"]);
    let one_off_sale = str_of(&v.body["sale_id"]);
    assert_eq!(s.remaining(&sold.item).await?, 11);

    let voided = s
        .void_visit(&s.manager, &visit, json!("  Ghi nhầm khách  "))
        .await;
    assert_eq!(voided.status, StatusCode::OK, "{}", voided.body);
    assert_eq!(voided.body["status"], "voided");
    assert_eq!(voided.body["void_reason"], "Ghi nhầm khách");
    assert!(voided.body["voided_at"].is_string());
    assert_eq!(
        s.remaining(&sold.item).await?,
        12,
        "AC6.1: the session is back"
    );

    // The money: the original payment stays, a mirroring reversal is added.
    let payments = voided.body["payments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(payments.len(), 2, "{payments:?}");
    assert_eq!(payments[0]["kind"], "payment");
    assert_eq!(payments[1]["kind"], "reversal");
    assert_eq!(payments[1]["reversal_of"], payments[0]["id"]);
    assert_eq!(payments[1]["amount"], 49_000);
    let sale = s.get(&format!("/sales/{one_off_sale}")).await;
    assert_eq!(sale.body["status"], "voided");
    assert_eq!(sale.body["amount_paid"], 0);
    assert_eq!(sale.body["void_reason"], "Ghi nhầm khách");

    // The visit stays in history with its status and reason.
    let today = s.get("/visits").await;
    let listed = today.body.as_array().cloned().unwrap_or_default();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["status"], "voided");
    assert_eq!(listed[0]["void_reason"], "Ghi nhầm khách");
    assert_eq!(
        s.count("SELECT count(*) FROM app.visit_line").await?,
        2,
        "lines are kept"
    );
    assert_eq!(
        s.count(
            "SELECT count(*) FROM app.audit_log WHERE action = 'visit.void' AND reason = 'Ghi nhầm khách'"
        )
        .await?,
        1
    );
    assert_eq!(
        s.count("SELECT count(*) FROM app.audit_log WHERE action = 'sale.void'")
            .await?,
        1
    );
    Ok(())
}

/// A void gives back the sessions of its lines whatever they were. After 10 paid + 1 bonus
/// sessions, voiding a paid visit leaves 1 paid + 1 bonus: the next visit is worth a paid session
/// (its performer's commission), the one after it is the free one. Red before the fix: the next
/// visit was valued 0, so 10 paid sessions were recognised as 9 × 150.000đ.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_paid_session_given_back_by_a_void_keeps_its_value(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let mut visits = Vec::new();
    for _ in 0..11 {
        visits.push(str_of(&s.visit(&sold.item, false).await.body["id"]));
    }
    let pool = &s.h.owner;
    assert_eq!(line_value(pool, &visits[0]).await?, 150_000);
    assert_eq!(line_value(pool, &visits[10]).await?, 0, "11th = bonus");

    let voided = s
        .void_visit(&s.manager, &visits[0], json!("Ghi nhầm"))
        .await;
    assert_eq!(voided.status, StatusCode::OK, "{}", voided.body);
    assert_eq!(s.remaining(&sold.item).await?, 2);

    let again = str_of(&s.visit(&sold.item, false).await.body["id"]);
    assert_eq!(
        line_value(pool, &again).await?,
        150_000,
        "the paid session given back"
    );
    let last = str_of(&s.visit(&sold.item, false).await.body["id"]);
    assert_eq!(
        line_value(pool, &last).await?,
        0,
        "the second bonus session"
    );
    let recognised: i64 = sqlx::query_scalar(
        "SELECT sum(l.line_value)::bigint FROM app.visit_line l
         JOIN app.visit v ON v.id = l.visit_id
         WHERE l.customer_package_item_id = $1::uuid AND v.status = 'completed'",
    )
    .bind(&sold.item)
    .fetch_one(pool)
    .await?;
    assert_eq!(recognised, 1_500_000, "10 paid sessions × 150.000đ");
    Ok(())
}

async fn line_value(pool: &PgPool, visit: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT line_value FROM app.visit_line WHERE visit_id = $1::uuid")
        .bind(visit)
        .fetch_one(pool)
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_visit_is_voided_once(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, true).await.body["id"]);
    let first = s.void_visit(&s.manager, &visit, json!("nhầm")).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    let second = s.void_visit(&s.owner, &visit, json!("nhầm lần 2")).await;
    assert_eq!(code(&second), (StatusCode::CONFLICT, Some("visit.voided")));
    assert_eq!(s.remaining(&sold.item).await?, 12, "credited once");
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment WHERE kind = 'reversal'")
            .await?,
        1
    );

    // A retry of the first void (same key, same body) replays it instead of failing.
    let key = Uuid::now_v7().to_string();
    let other = str_of(&s.visit(&sold.item, false).await.body["id"]);
    let path = format!("/visits/{other}/void");
    let body = json!({"reason": "khách đổi ý"});
    let a = s
        .post_with(
            &s.manager,
            &path,
            body.clone(),
            &[("idempotency-key", &key)],
        )
        .await;
    let b = s
        .post_with(&s.manager, &path, body, &[("idempotency-key", &key)])
        .await;
    assert_eq!((a.status, b.status), (StatusCode::OK, StatusCode::OK));
    assert_eq!(a.body["voided_at"], b.body["voided_at"]);
    assert_eq!(s.remaining(&sold.item).await?, 12);

    // The same key and body on another visit is not a retry: it must not replay the first void.
    let third = str_of(&s.visit(&sold.item, false).await.body["id"]);
    let reused = s
        .post_with(
            &s.manager,
            &format!("/visits/{third}/void"),
            json!({"reason": "khách đổi ý"}),
            &[("idempotency-key", &key)],
        )
        .await;
    assert_eq!(
        code(&reused),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("idempotency.key_reuse_mismatch")
        )
    );
    assert_eq!(s.remaining(&sold.item).await?, 11, "third visit untouched");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn two_voids_of_one_visit_in_parallel_exactly_one_wins(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, true).await.body["id"]);
    let (a, b) = tokio::join!(
        s.void_visit(&s.manager, &visit, json!("nhầm")),
        s.void_visit(&s.owner, &visit, json!("nhầm"))
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::CONFLICT],
        "{} / {}",
        a.body,
        b.body
    );
    let loser = if a.status == StatusCode::CONFLICT {
        &a
    } else {
        &b
    };
    assert_eq!(loser.body["code"], "visit.voided");
    assert_eq!(s.remaining(&sold.item).await?, 12, "credited exactly once");
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment WHERE kind = 'reversal'")
            .await?,
        1
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_void_needs_a_reason(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, false).await.body["id"]);
    for reason in [Value::Null, json!(""), json!("   ")] {
        let r = s.void_visit(&s.manager, &visit, reason).await;
        assert_eq!(
            code(&r),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("validation_failed")),
            "AC6.2"
        );
        assert_eq!(r.body["errors"][0]["field"], "reason");
    }
    let no_body = s
        .post(&s.manager, &format!("/visits/{visit}/void"), json!({}))
        .await;
    assert_eq!(no_body.status, StatusCode::UNPROCESSABLE_ENTITY);
    let sale = s.void_sale(&s.manager, &sold.sale, json!("")).await;
    assert_eq!(sale.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(s.remaining(&sold.item).await?, 11, "nothing changed");
    let no_key = call(
        &s.h.app,
        "POST",
        &format!("/visits/{visit}/void"),
        Some(&s.manager),
        Some(json!({"reason": "nhầm"})),
    )
    .await;
    assert_eq!(
        code(&no_key),
        (StatusCode::BAD_REQUEST, Some("idempotency.key_required"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn closed_days_are_locked_for_managers_and_open_to_owners_without_reauth(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, true).await.body["id"]);
    s.exec("UPDATE app.visit SET business_date = business_date - 1")
        .await?;

    let manager = s.void_visit(&s.manager, &visit, json!("nhầm")).await;
    assert_eq!(
        code(&manager),
        (StatusCode::FORBIDDEN, Some("day.locked")),
        "AC6.3"
    );

    assert_eq!(s.remaining(&sold.item).await?, 11, "nothing changed");

    s.age_reauth().await?;
    let owner = s.void_visit(&s.owner, &visit, json!("nhầm hôm qua")).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(s.remaining(&sold.item).await?, 12);

    // Same rule for a package sale of a past day.
    let other = s.sell().await?;
    s.exec(&format!(
        "UPDATE app.sale SET business_date = business_date - 1 WHERE id = '{}'",
        other.sale
    ))
    .await?;
    let manager_sale = s.void_sale(&s.manager, &other.sale, json!("nhầm")).await;
    assert_eq!(
        code(&manager_sale),
        (StatusCode::FORBIDDEN, Some("day.locked"))
    );
    let owner_sale = s.void_sale(&s.owner, &other.sale, json!("nhầm")).await;
    assert_eq!(owner_sale.status, StatusCode::OK, "{}", owner_sale.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn same_day_voids_need_no_reauth(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, false).await.body["id"]);
    s.age_reauth().await?;
    let r = s.void_visit(&s.owner, &visit, json!("nhầm")).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    Ok(())
}

/// S10: correcting a closed day's visit (`PATCH /visits/{id}`) follows the same rule as voiding
/// it: managers get `403 day.locked`, owners may, without re-entering the password.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn correcting_a_closed_day_visit_follows_the_void_rule(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, false).await.body["id"]);
    s.exec("UPDATE app.visit SET business_date = business_date - 1")
        .await?;
    let patch = |who: String| {
        let app = s.h.app.clone();
        let path = format!("/visits/{visit}");
        async move {
            call_with_headers(
                &app,
                "PATCH",
                &path,
                Some(&who),
                Some(json!({"note": "Khách thích lực nhẹ"})),
                &[("If-Match", "\"1\"")],
            )
            .await
        }
    };

    let manager = patch(s.manager.clone()).await;
    assert_eq!(code(&manager), (StatusCode::FORBIDDEN, Some("day.locked")));

    assert_eq!(
        s.count("SELECT count(*) FROM app.audit_log WHERE action = 'visit.update'")
            .await?,
        0,
        "nothing changed"
    );

    s.age_reauth().await?;
    let owner = patch(s.owner.clone()).await;
    assert_eq!(owner.status, StatusCode::OK, "{}", owner.body);
    assert_eq!(owner.body["version"], 2);
    assert_eq!(
        s.count("SELECT count(*) FROM app.audit_log WHERE action = 'visit.update'")
            .await?,
        1,
        "audited"
    );

    // Today's visits, as for voids.
    let today = str_of(&s.visit(&sold.item, false).await.body["id"]);
    s.age_reauth().await?;
    let same_day = call_with_headers(
        &s.h.app,
        "PATCH",
        &format!("/visits/{today}"),
        Some(&s.owner),
        Some(json!({"note": null})),
        &[("If-Match", "\"1\"")],
    )
    .await;
    assert_eq!(same_day.status, StatusCode::OK, "{}", same_day.body);
    Ok(())
}

// --------------------------------------------------------------------------- sales (US6)

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn voiding_an_unused_package_sale_reverses_it_and_cancels_the_package(
    pool: PgPool,
) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let r = s
        .void_sale(&s.manager, &sold.sale, json!("bán nhầm gói"))
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["status"], "voided");
    assert_eq!(r.body["amount_paid"], 0);
    assert_eq!(r.body["void_reason"], "bán nhầm gói");
    let payments = r.body["payments"].as_array().cloned().unwrap_or_default();
    assert_eq!(payments.len(), 2);
    assert_eq!(payments[1]["kind"], "reversal");
    assert_eq!(payments[1]["amount"], 1_500_000);
    let package = &r.body["customer_packages"][0];
    assert_eq!(package["status"], "cancelled");
    assert_eq!(package["sessions_remaining"], 0);
    assert_eq!(package["cancel_reason"], "bán nhầm gói");
    assert_eq!(
        s.count("SELECT count(*) FROM app.sale_line").await?,
        1,
        "kept"
    );

    let again = s
        .void_sale(&s.owner, &sold.sale, json!("bán nhầm gói"))
        .await;
    assert_eq!(code(&again), (StatusCode::CONFLICT, Some("sale.voided")));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_package_sale_with_used_sessions_cannot_be_voided(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, false).await.body["id"]);
    let blocked = s.void_sale(&s.manager, &sold.sale, json!("bán nhầm")).await;
    assert_eq!(
        code(&blocked),
        (StatusCode::CONFLICT, Some("sale.has_used_sessions")),
        "AC6.4"
    );
    assert_eq!(s.remaining(&sold.item).await?, 11);
    assert_eq!(
        s.count("SELECT count(*) FROM app.payment WHERE kind = 'reversal'")
            .await?,
        0
    );

    // Once the visit is voided, nothing was used and the sale can go.
    let v = s.void_visit(&s.manager, &visit, json!("nhầm")).await;
    assert_eq!(v.status, StatusCode::OK);
    let ok = s.void_sale(&s.manager, &sold.sale, json!("bán nhầm")).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_visits_own_sale_is_voided_through_the_visit(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let v = s.visit(&sold.item, true).await;
    let r = s
        .void_sale(&s.manager, &str_of(&v.body["sale_id"]), json!("nhầm"))
        .await;
    assert_eq!(
        code(&r),
        (StatusCode::CONFLICT, Some("sale.void_via_visit"))
    );
    Ok(())
}

/// The append-only walls behind the service: a payment is reversed at most once, a reversal is
/// never reversed, money rows never change, voided records never come back.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_reversal_cannot_be_reversed_or_repeated(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let r = s.void_sale(&s.manager, &sold.sale, json!("nhầm")).await;
    assert_eq!(r.status, StatusCode::OK);
    let original = str_of(&r.body["payments"][0]["id"]);
    let reversal = str_of(&r.body["payments"][1]["id"]);
    let app = app_pool(&s.h.owner).await?;
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&s.h.owner)
        .await?;
    let insert = |of: String| {
        let app = app.clone();
        async move {
            let mut tx = app.begin().await?;
            sqlx::query("SELECT set_config('app.tenant_id', $1, true)")
                .bind(tenant.to_string())
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO app.payment (id, tenant_id, branch_id, sale_id, business_date, kind, method, amount, reversal_of, created_by)
                 SELECT $1, tenant_id, branch_id, sale_id, business_date, 'reversal', method, amount, id, created_by
                 FROM app.payment WHERE id = $2::uuid",
            )
            .bind(Uuid::now_v7())
            .bind(of)
            .execute(&mut *tx)
            .await
        }
    };
    let twice = insert(original.clone()).await;
    assert!(
        twice
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .is_some_and(|e| e.is_unique_violation()),
        "second reversal of one payment: {twice:?}"
    );
    let of_reversal = insert(reversal).await;
    assert!(
        of_reversal
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .is_some_and(|e| e.is_check_violation()),
        "reversal of a reversal: {of_reversal:?}"
    );
    let changed = s
        .exec(&format!(
            "UPDATE app.payment SET amount = 1 WHERE id = '{original}'"
        ))
        .await;
    assert!(
        changed.is_err(),
        "payments are append-only, even for the owner"
    );
    let reopened = s
        .exec(&format!(
            "UPDATE app.sale SET status = 'open' WHERE id = '{}'",
            sold.sale
        ))
        .await;
    assert!(reopened.is_err(), "voided is final");
    Ok(())
}

// --------------------------------------------------------------------- packages (US7)

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_extends_an_expired_package_with_a_reason(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    s.exec(&format!(
        "UPDATE app.customer_package SET starts_on = current_date - 200, expires_on = current_date - 10 WHERE id = '{}'",
        sold.package
    ))
    .await?;
    s.exec(&format!(
        "UPDATE app.customer_package_item SET sessions_remaining = 3 WHERE id = '{}'",
        sold.item
    ))
    .await?;
    let before = s.get(&format!("/customer-packages/{}", sold.package)).await;
    assert_eq!(before.body["status"], "expired");
    let version = before.body["version"].to_string();
    let new_expiry = (chrono::Utc::now().date_naive() + chrono::Days::new(60)).to_string();
    let body = json!({"expires_on": new_expiry, "reason": "Khách nằm viện 2 tháng"});

    let manager = s
        .extend(&s.manager, &sold.package, &version, body.clone())
        .await;
    assert_eq!(
        code(&manager),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "AC7.3"
    );

    let stale = s.extend(&s.owner, &sold.package, "99", body.clone()).await;
    assert_eq!(stale.status, StatusCode::PRECONDITION_FAILED);
    let no_reason = s
        .extend(
            &s.owner,
            &sold.package,
            &version,
            json!({"expires_on": new_expiry}),
        )
        .await;
    assert_eq!(no_reason.body["errors"][0]["field"], "reason");
    let earlier = s
        .extend(
            &s.owner,
            &sold.package,
            &version,
            json!({"expires_on": "2020-01-01", "reason": "x"}),
        )
        .await;
    assert_eq!(earlier.body["errors"][0]["code"], "not_later");

    let ok = s.extend(&s.owner, &sold.package, &version, body).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    assert_eq!(ok.body["status"], "active", "AC7.1");
    assert_eq!(ok.body["expires_on"], new_expiry);
    assert_eq!(ok.body["sessions_remaining"], 3);
    assert!(ok.headers.get("etag").is_some());
    let audited: (Value, Value, String) = sqlx::query_as(
        "SELECT before, after, reason FROM app.audit_log WHERE action = 'customer_package.extend'",
    )
    .fetch_one(&s.h.owner)
    .await?;
    assert_eq!(audited.1["expires_on"], new_expiry);
    assert!(audited.0["expires_on"].is_string());
    assert_eq!(audited.2, "Khách nằm viện 2 tháng");
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn owner_cancels_a_package_with_a_refund(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let visit = str_of(&s.visit(&sold.item, false).await.body["id"]);
    let refund =
        json!({"reason": "Khách chuyển nhà", "refund_amount": 300_000, "refund_method": "cash"});

    let manager = s.cancel(&s.manager, &sold.package, refund.clone()).await;
    assert_eq!(
        code(&manager),
        (StatusCode::FORBIDDEN, Some("auth.forbidden")),
        "AC7.3"
    );

    let too_much = s
        .cancel(
            &s.owner,
            &sold.package,
            json!({"reason": "x", "refund_amount": 1_500_001, "refund_method": "cash"}),
        )
        .await;
    assert_eq!(
        code(&too_much),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("refund.exceeds_paid")
        ),
        "AC7.4"
    );

    // No password prompt for the owner (EM decision 2026-11-05).
    s.age_reauth().await?;
    let ok = s.cancel(&s.owner, &sold.package, refund.clone()).await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
    assert_eq!(ok.body["status"], "cancelled", "AC7.2");
    assert_eq!(ok.body["sessions_remaining"], 0);
    assert_eq!(ok.body["cancel_reason"], "Khách chuyển nhà");
    assert_eq!(ok.body["refund"]["amount"], 300_000);
    assert_eq!(ok.body["refund"]["method"], "cash");
    assert_eq!(s.count("SELECT count(*) FROM app.refund").await?, 1);
    assert_eq!(
        s.count("SELECT count(*) FROM app.audit_log WHERE action = 'customer_package.cancel' AND reason = 'Khách chuyển nhà'")
            .await?,
        1
    );

    let twice = s.cancel(&s.owner, &sold.package, refund).await;
    assert_eq!(
        code(&twice),
        (StatusCode::CONFLICT, Some("package.cancelled"))
    );
    // No way back to money or sessions through a void either.
    let void_visit = s.void_visit(&s.manager, &visit, json!("nhầm")).await;
    assert_eq!(
        code(&void_visit),
        (StatusCode::CONFLICT, Some("package.cancelled"))
    );
    assert_eq!(s.remaining(&sold.item).await?, 0);
    let void_sale = s.void_sale(&s.manager, &sold.sale, json!("nhầm")).await;
    assert_eq!(
        code(&void_sale),
        (StatusCode::CONFLICT, Some("package.cancelled")),
        "the refund already returned money"
    );
    let extend = s
        .extend(
            &s.owner,
            &sold.package,
            &ok.body["version"].to_string(),
            json!({"expires_on": "2099-01-01", "reason": "x"}),
        )
        .await;
    assert_eq!(
        code(&extend),
        (StatusCode::CONFLICT, Some("package.cancelled"))
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn refund_is_capped_by_what_was_paid_after_discount(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s
        .post(
            &s.owner,
            "/sales",
            json!({
                "customer_id": s.ha, "closer_staff_id": s.lan,
                "lines": [{"package_plan_id": s.plan}],
                "discount": {"amount": 300_000, "reason": "khách quen"},
                "payments": [{"method": "bank_transfer", "amount": 1_200_000}],
            }),
        )
        .await;
    assert_eq!(sold.status, StatusCode::CREATED, "{}", sold.body);
    let package = str_of(&sold.body["customer_packages"][0]["id"]);
    let over = s
        .cancel(
            &s.owner,
            &package,
            json!({"reason": "x", "refund_amount": 1_200_001, "refund_method": "bank_transfer"}),
        )
        .await;
    assert_eq!(
        code(&over),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Some("refund.exceeds_paid")
        )
    );
    let all = s
        .cancel(
            &s.owner,
            &package,
            json!({"reason": "x", "refund_amount": 1_200_000, "refund_method": "bank_transfer"}),
        )
        .await;
    assert_eq!(all.status, StatusCode::OK, "{}", all.body);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn cancelling_without_refund_records_no_money(pool: PgPool) -> TestResult {
    let s = spa(pool).await?;
    let sold = s.sell().await?;
    let r = s
        .cancel(
            &s.owner,
            &sold.package,
            json!({"reason": "khách tặng lại buổi"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.body);
    assert_eq!(r.body["refund"], Value::Null);
    assert_eq!(s.count("SELECT count(*) FROM app.refund").await?, 0);
    let method_needed = s
        .cancel(
            &s.owner,
            &s.sell().await?.package,
            json!({"reason": "x", "refund_amount": 1}),
        )
        .await;
    assert_eq!(method_needed.body["errors"][0]["field"], "refund_method");
    Ok(())
}
