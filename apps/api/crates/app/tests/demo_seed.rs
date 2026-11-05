//! `app seed-demo`: a fresh spa gets realistic data on every R1-1 screen, through the real use
//! cases (audit, append-only money), and a second run refuses to touch it.

mod common;

use app::demo::{DemoOptions, DemoReport, MANAGER_PHONE, seed};
use common::{PASSWORD, TestResult, call, harness, session_for};
use kernel::testing::app_pool;
use serde_json::Value;
use sqlx::PgPool;

const OPTIONS: DemoOptions<'static> = DemoOptions {
    tenant_slug: "vpyd",
    manager_password: PASSWORD,
    force_again: false,
};

async fn count(owner: &PgPool, sql: &'static str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sql).fetch_one(owner).await
}

fn list(reply: &Value) -> Vec<Value> {
    reply.as_array().cloned().unwrap_or_default()
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_fresh_spa_gets_demo_data_on_every_screen_once(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let app = app_pool(&h.owner).await?;

    let report: DemoReport = seed(&h.owner, &app, &h.cipher, &OPTIONS).await?;

    assert_eq!(report.run, 0);
    assert!(report.catalog_seeded, "an empty spa gets the printed menu");
    assert_eq!(report.plans_created, 5);
    assert_eq!(report.staff_added, 6, "5 technicians + the manager");
    assert_eq!(report.customers, 40);
    assert_eq!(report.health_notes, 14);
    assert_eq!(report.opening_balances, 3);
    assert_eq!(
        report.package_sales, 22,
        "18 past, 2 today, 1 voided + its replacement"
    );
    assert!(report.discounted_sales > 0);
    assert!(
        report.visits >= 80,
        "three weeks of visits: {}",
        report.visits
    );
    assert!((8..=15).contains(&report.visits_today), "{report:?}");
    assert_eq!(
        (
            report.voided_sales,
            report.voided_visits,
            report.cancelled_with_refund
        ),
        (1, 1, 1)
    );

    // What the database holds matches, and every write left its audit entry.
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer").await?,
        40
    );
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.customer WHERE health_note_ct IS NOT NULL"
        )
        .await?,
        14
    );
    let visits = count(&h.owner, "SELECT count(*) FROM app.visit").await?;
    assert_eq!(visits, i64::try_from(report.visits)?);
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.audit_log WHERE action = 'visit.create'"
        )
        .await?,
        visits
    );
    assert!(
        count(
            &h.owner,
            "SELECT count(DISTINCT business_date) FROM app.visit"
        )
        .await?
            >= 20,
        "history spans the past three weeks"
    );
    assert!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.payment WHERE kind = 'reversal'"
        )
        .await?
            >= 1,
        "voids reverse money instead of deleting it"
    );
    assert_eq!(count(&h.owner, "SELECT count(*) FROM app.refund").await?, 1);

    // The demo manager logs in and the front desk has something to show.
    let manager = session_for(&h, MANAGER_PHONE).await;
    assert!(!manager.is_empty(), "manager login works without MFA");
    let today = list(
        &call(&h.app, "GET", "/visits", Some(&manager), None)
            .await
            .body,
    );
    assert!(
        (8..=15).contains(&today.len()),
        "{} visits today",
        today.len()
    );
    assert!(today.iter().any(|v| v["status"] == "voided"));
    assert!(
        today
            .iter()
            .flat_map(|v| v["lines"].as_array().cloned().unwrap_or_default())
            .any(|l| l["expiry_override_reason"].is_string()),
        "an expired package used with an override"
    );

    let found = list(
        &call(
            &h.app,
            "GET",
            "/customers?q=0399000001",
            Some(&manager),
            None,
        )
        .await
        .body,
    );
    let mai = found.first().map(|c| c["id"].clone()).unwrap_or_default();
    let mai = mai.as_str().unwrap_or_default();
    let profile = call(
        &h.app,
        "GET",
        &format!("/customers/{mai}"),
        Some(&manager),
        None,
    )
    .await;
    assert_eq!(profile.body["health_note"], "Huyết áp cao — tránh đá nóng");
    let packages = list(
        &call(
            &h.app,
            "GET",
            &format!("/customers/{mai}/packages"),
            Some(&manager),
            None,
        )
        .await
        .body,
    );
    // Migrated packages look like any other package: no paper-book flag, just no sale.
    assert!(
        packages
            .iter()
            .all(|p| p.get("origin").is_none() && p["sale_id"].is_null())
    );
    assert!(packages.iter().any(|p| p["status"] == "expired"));
    assert!(
        packages
            .iter()
            .any(|p| p["status"] == "active" && p["expires_on"].is_string()),
        "a package expiring soon: {packages:?}"
    );

    // A second run changes nothing.
    let again = seed(&h.owner, &app, &h.cipher, &OPTIONS).await;
    let message = again.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("already has demo data"), "{message}");
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer").await?,
        40
    );
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.visit").await?,
        visits
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn force_again_adds_a_second_batch_on_a_new_phone_block(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    let app = app_pool(&h.owner).await?;
    seed(&h.owner, &app, &h.cipher, &OPTIONS).await?;

    let second = seed(
        &h.owner,
        &app,
        &h.cipher,
        &DemoOptions {
            force_again: true,
            ..OPTIONS
        },
    )
    .await?;

    assert_eq!(second.run, 1);
    assert_eq!(
        (second.plans_created, second.staff_added, second.customers),
        (0, 0, 40),
        "staff and plans are reused"
    );
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.customer WHERE phone_e164 LIKE '+8439901%'"
        )
        .await?,
        40
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_spa_without_an_owner_is_refused_before_anything_is_written(pool: PgPool) -> TestResult {
    identity::provisioning::create_tenant(&pool, "trong", "Spa trống", "Cơ sở chính").await?;
    let app = app_pool(&pool).await?;
    let cipher = kernel::crypto::FieldCipher::from_base64(common::TEST_DATA_KEY)?;

    let refused = seed(
        &pool,
        &app,
        &cipher,
        &DemoOptions {
            tenant_slug: "trong",
            ..OPTIONS
        },
    )
    .await;

    let message = refused.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("no owner"), "{message}");
    assert_eq!(count(&pool, "SELECT count(*) FROM app.customer").await?, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM app.staff").await?, 0);
    Ok(())
}
