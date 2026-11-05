//! `app import-paper-book` (spec management-views §7): the team migrates the paper package books
//! through the owner's use case; a second run imports nothing; a bad file writes nothing.

mod common;

use app::paper_book::{ImportOptions, ImportReport, import};
use catalog::service::seed_menu;
use common::{OWNER_PHONE, TestResult, call, harness, session_for};
use kernel::testing::app_pool;
use serde_json::{Value, json};
use sqlx::PgPool;

const HEADER: &str = "phone,full_name,gender,package_name,sold_price,starts_on,expires_on,gift_note,service,sessions_purchased,sessions_bonus,sessions_remaining";

fn csv(rows: &[&str]) -> String {
    format!("{HEADER}\n{}\n", rows.join("\n"))
}

async fn count(owner: &PgPool, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(owner)
        .await
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_paper_books_import_once_through_the_owner_use_case(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let app = app_pool(&h.owner).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let existing = call(
        &h.app,
        "POST",
        "/customers",
        Some(&owner),
        Some(json!({"full_name": "Nguyễn Thị Hà", "phone": "0905111001", "gender": "female"})),
    )
    .await;
    let existing_id = existing.body["id"].clone();
    let file = csv(&[
        "0905111001,,,Gội đầu 10 buổi,1500000,2026-06-01,,Khăn tắm,cơ bản – dầu thảo dược,10,2,4",
        "0905222002,\"Trần Thị Lan, chị\",female,Combo,2000000,2026-05-01,2027-05-01,,Cơ Bản – Dầu thảo dược,5,0,5",
        "0905 222 002,,,Combo,2000000,2026-05-01,2027-05-01,,Thư Giãn,5,0,3",
    ]);
    let options = |dry_run| ImportOptions {
        tenant_slug: "vpyd",
        csv: &file,
        dry_run,
    };

    let dry = import(&h.owner, &app, &h.cipher, &options(true)).await?;
    assert_eq!(
        dry,
        ImportReport {
            rows: 3,
            packages: 2,
            customers: 2,
            customers_created: 1,
            imported: 2,
            skipped_existing: 0
        }
    );
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer_package").await?,
        0,
        "dry run writes nothing"
    );

    let report = import(&h.owner, &app, &h.cipher, &options(false)).await?;
    assert_eq!(report, dry);
    // No sale, no payment: never revenue. Audited as the owner, like the front desk import.
    for table in ["sale", "payment"] {
        assert_eq!(
            count(&h.owner, &format!("SELECT count(*) FROM app.{table}")).await?,
            0
        );
    }
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.audit_log WHERE action = 'opening_balance.import'"
        )
        .await?,
        2
    );
    let packages = call(
        &h.app,
        "GET",
        &format!(
            "/customers/{}/packages",
            existing_id.as_str().unwrap_or_default()
        ),
        Some(&owner),
        None,
    )
    .await;
    let first = &packages.body[0];
    assert_eq!(
        (
            first["sessions_remaining"].clone(),
            first["sessions_total"].clone(),
            first["gift_note"].clone(),
            first["sale_id"].clone()
        ),
        (json!(4), json!(12), json!("Khăn tắm"), Value::Null)
    );
    assert!(first.get("origin").is_none(), "no paper-book label");
    let lan: String =
        sqlx::query_scalar("SELECT full_name FROM app.customer WHERE phone_e164 = '+84905222002'")
            .fetch_one(&h.owner)
            .await?;
    assert_eq!(lan, "Trần Thị Lan, chị");
    let combo_items = count(
        &h.owner,
        "SELECT count(*) FROM app.customer_package_item i JOIN app.customer_package p ON p.id = i.customer_package_id WHERE p.name_snapshot = 'Combo'",
    )
    .await?;
    assert_eq!(combo_items, 2, "two rows, one package");

    // Running the same file again imports nothing.
    let again = import(&h.owner, &app, &h.cipher, &options(false)).await?;
    assert_eq!((again.imported, again.skipped_existing), (0, 2));
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer_package").await?,
        2
    );
    Ok(())
}

#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn a_file_with_a_mistake_writes_nothing(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let app = app_pool(&h.owner).await?;
    let unknown_service = csv(&[
        "0905111001,Hà,,Gói A,1500000,2026-06-01,,,Cơ Bản – Dầu thảo dược,10,0,4",
        "0905222002,Lan,,Gói B,1500000,2026-06-01,,,Không có dịch vụ này,10,0,4",
    ]);
    let error = import(
        &h.owner,
        &app,
        &h.cipher,
        &ImportOptions {
            tenant_slug: "vpyd",
            csv: &unknown_service,
            dry_run: false,
        },
    )
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(error.contains("line 3: no service named"), "{error}");
    let no_name = csv(&["0905333003,,,Gói A,1500000,2026-06-01,,,Cơ Bản – Dầu thảo dược,10,0,4"]);
    let error = import(
        &h.owner,
        &app,
        &h.cipher,
        &ImportOptions {
            tenant_slug: "vpyd",
            csv: &no_name,
            dry_run: false,
        },
    )
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(error.contains("full_name is required"), "{error}");
    let too_many =
        csv(&["0905111001,Hà,,Gói A,1500000,2026-06-01,,,Cơ Bản – Dầu thảo dược,10,0,11"]);
    let error = import(
        &h.owner,
        &app,
        &h.cipher,
        &ImportOptions {
            tenant_slug: "vpyd",
            csv: &too_many,
            dry_run: false,
        },
    )
    .await
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(
        error.contains("validation_failed"),
        "remaining > total is refused by the use case: {error}"
    );
    for table in ["customer", "customer_package"] {
        assert_eq!(
            count(&h.owner, &format!("SELECT count(*) FROM app.{table}")).await?,
            0,
            "{table}"
        );
    }
    assert!(
        import(
            &h.owner,
            &app,
            &h.cipher,
            &ImportOptions {
                tenant_slug: "nope",
                csv: &no_name,
                dry_run: true
            }
        )
        .await
        .is_err()
    );
    Ok(())
}
