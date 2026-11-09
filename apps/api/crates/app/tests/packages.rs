//! In-transaction package API used by S7 (consume a session) and S9 (opening balance).

mod common;

use axum::http::StatusCode;
use catalog::service::seed_menu;
use common::{OWNER_PHONE, TestResult, call, harness, session_for};
use kernel::testing::app_pool;
use kernel::{Actor, AppError, Money, Permission, TenantId, TenantTx, UserId};
use packages::{Consume, NewItem, NewPackage, Origin};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    app_pool: PgPool,
    owner: PgPool,
    actor: Actor,
    customer: Uuid,
    other_customer: Uuid,
    service: Uuid,
}

async fn fixture(pool: PgPool) -> Result<Fixture, Box<dyn std::error::Error>> {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let token = session_for(&h, OWNER_PHONE).await;
    let mut ids = Vec::new();
    for (name, phone) in [("Hà", "0905111222"), ("Lan", "0905333444")] {
        let c = call(
            &h.app,
            "POST",
            "/customers",
            Some(&token),
            Some(json!({"full_name": name, "phone": phone})),
        )
        .await;
        assert_eq!(c.status, StatusCode::CREATED);
        ids.push(Uuid::parse_str(c.body["id"].as_str().unwrap_or_default())?);
    }
    let tenant: Uuid = sqlx::query_scalar("SELECT id FROM app.tenant WHERE slug = 'vpyd'")
        .fetch_one(&h.owner)
        .await?;
    let service: Uuid =
        sqlx::query_scalar("SELECT id FROM app.service WHERE name = 'Cơ Bản – Dầu thảo dược'")
            .fetch_one(&h.owner)
            .await?;
    // A real account: who-did-it columns reference user_account (0036).
    let user: Uuid =
        sqlx::query_scalar("SELECT id FROM app.user_account ORDER BY created_at, id LIMIT 1")
            .fetch_one(&h.owner)
            .await?;
    Ok(Fixture {
        app_pool: app_pool(&h.owner).await?,
        owner: h.owner,
        actor: Actor::new(
            UserId::from_uuid(user),
            TenantId::from_uuid(tenant),
            [Permission::OpeningBalanceImport],
            None,
        ),
        customer: ids[0],
        other_customer: ids[1],
        service,
    })
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

impl Fixture {
    /// Opening balance "4 left of 10 + 2" (S9 path): no sale, no payment.
    async fn opening_balance(&self, remaining: u32) -> Result<packages::CustomerPackage, AppError> {
        let mut tx = TenantTx::begin(&self.app_pool, self.actor.tenant_id).await?;
        let p = packages::service::create(
            &mut tx,
            &self.actor,
            &NewPackage {
                customer_id: self.customer,
                plan_id: None,
                name: "Gội đầu (sổ cũ)".into(),
                gift_note: None,
                sold_price: Money::vnd(1_500_000).unwrap_or_default(),
                origin: Origin::OpeningBalance,
                starts_on: today(),
                expires_on: None,
                items: vec![NewItem {
                    service_id: self.service,
                    name: "Cơ Bản – Dầu thảo dược".into(),
                    sessions_purchased: 10,
                    sessions_bonus: 2,
                    sessions_remaining: remaining,
                }],
            },
        )
        .await?;
        tx.commit().await?;
        Ok(p)
    }

    fn consume(&self, item_id: Uuid, customer_id: Uuid) -> Consume {
        Consume {
            item_id,
            customer_id,
            qty: 1,
            today: today(),
            allow_expired: false,
        }
    }
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn opening_balance_creates_a_package_without_money(pool: PgPool) -> TestResult {
    let f = fixture(pool).await?;
    let p = f.opening_balance(4).await?;
    assert_eq!((p.sessions_remaining, p.sessions_total), (4, 12));
    assert_eq!((p.origin, p.sale_id), ("opening_balance", None));
    for table in ["sale", "payment"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM app.{table}"
        )))
        .fetch_one(&f.owner)
        .await?;
        assert_eq!(n, 0, "{table}");
    }
    let too_many = f.opening_balance(13).await;
    assert!(matches!(too_many, Err(AppError::Validation(_))));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn consuming_counts_down_and_values_paid_sessions_first(pool: PgPool) -> TestResult {
    let f = fixture(pool).await?;
    let p = f.opening_balance(3).await?;
    let item = p.items[0].id;
    // 12 total, 3 left: 9 used, so the next one is the 10th (last paid), then two bonus ones.
    let mut values = Vec::new();
    for _ in 0..3 {
        let mut tx = TenantTx::begin(&f.app_pool, f.actor.tenant_id).await?;
        let c = packages::service::consume_session(&mut tx, f.consume(item, f.customer)).await?;
        tx.commit().await?;
        values.push((c.sessions_remaining, c.value.as_i64()));
    }
    assert_eq!(values, vec![(2, 150_000), (1, 0), (0, 0)]);

    let mut tx = TenantTx::begin(&f.app_pool, f.actor.tenant_id).await?;
    let empty = packages::service::consume_session(&mut tx, f.consume(item, f.customer)).await;
    assert!(matches!(
        empty,
        Err(AppError::Conflict {
            code: "package.no_sessions_left"
        })
    ));
    let not_hers =
        packages::service::consume_session(&mut tx, f.consume(item, f.other_customer)).await;
    assert!(matches!(
        not_hers,
        Err(AppError::Unprocessable {
            code: "package.item_not_found"
        })
    ));
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn expired_packages_need_an_override(pool: PgPool) -> TestResult {
    let f = fixture(pool).await?;
    let p = f.opening_balance(5).await?;
    sqlx::query("UPDATE app.customer_package SET starts_on = current_date - 60, expires_on = current_date - 1")
        .execute(&f.owner)
        .await?;
    let mut tx = TenantTx::begin(&f.app_pool, f.actor.tenant_id).await?;
    let blocked =
        packages::service::consume_session(&mut tx, f.consume(p.items[0].id, f.customer)).await;
    assert!(matches!(
        blocked,
        Err(AppError::Conflict {
            code: "package.expired"
        })
    ));
    let mut overridden = f.consume(p.items[0].id, f.customer);
    overridden.allow_expired = true;
    let ok = packages::service::consume_session(&mut tx, overridden).await?;
    assert_eq!(ok.sessions_remaining, 4);
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn two_devices_racing_for_the_last_session_exactly_one_wins(pool: PgPool) -> TestResult {
    let f = fixture(pool).await?;
    let item = f.opening_balance(1).await?.items[0].id;

    // Device A takes the last session and keeps its transaction open…
    let mut a = TenantTx::begin(&f.app_pool, f.actor.tenant_id).await?;
    packages::service::consume_session(&mut a, f.consume(item, f.customer)).await?;
    // …device B tries meanwhile and blocks on the row lock.
    let (pool_b, tenant, request) = (
        f.app_pool.clone(),
        f.actor.tenant_id,
        f.consume(item, f.customer),
    );
    let b = tokio::spawn(async move {
        let mut tx = TenantTx::begin(&pool_b, tenant).await?;
        let result = packages::service::consume_session(&mut tx, request).await;
        tx.commit().await?;
        result
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(!b.is_finished(), "B waits for A's row lock");
    a.commit().await?;
    let b = b.await?;
    assert!(
        matches!(
            b,
            Err(AppError::Conflict {
                code: "package.no_sessions_left"
            })
        ),
        "AC4.7: {b:?}"
    );
    let left: i32 = sqlx::query_scalar("SELECT sessions_remaining FROM app.customer_package_item")
        .fetch_one(&f.owner)
        .await?;
    assert_eq!(left, 0, "never negative");
    Ok(())
}
