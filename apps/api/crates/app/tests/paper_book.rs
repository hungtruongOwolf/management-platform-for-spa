//! `app import-paper-book` (spec management-views §7): the team migrates the paper package books
//! through the owner's use case; a second run imports nothing; a bad file writes nothing.

mod common;

use app::paper_book::{ImportOptions, ImportReport, import};
use catalog::service::seed_menu;
use common::{OWNER_PHONE, TestResult, call, harness, post_customer, session_for};
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
    let existing = post_customer(
        &h.app,
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
            skipped_existing: 0,
            payments_booked: 0
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

const PAID_HEADER: &str = "phone,full_name,gender,package_name,sold_price,starts_on,expires_on,gift_note,service,sessions_purchased,sessions_bonus,sessions_remaining,paid_amount,paid_on,method";

fn days_ago(n: i64) -> chrono::NaiveDate {
    chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Ho_Chi_Minh)
        .date_naive()
        - chrono::Duration::days(n)
}

async fn run(
    h: &common::Harness,
    app: &PgPool,
    file: &str,
) -> Result<ImportReport, Box<dyn std::error::Error>> {
    Ok(import(
        &h.owner,
        app,
        &h.cipher,
        &ImportOptions {
            tenant_slug: "vpyd",
            csv: file,
            dry_run: false,
        },
    )
    .await?)
}

/// EM 2026-11-05 ("cứ tính là tiền"): money paid on paper is booked as a sale on the paper date,
/// so it is revenue of that day everywhere; re-runs never book it twice; an already imported
/// package can get its paid amount later, once.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn paper_money_is_revenue_on_the_paper_date(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let app = app_pool(&h.owner).await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let paid_on = days_ago(40);
    let (start, later) = (days_ago(45), days_ago(10));
    let file = format!(
        "{PAID_HEADER}\n\
         0905111001,Nguyễn Thị Hà,female,Gội đầu 10 buổi,1500000,{start},,,Cơ Bản – Dầu thảo dược,10,0,6,1500000,{paid_on},transfer\n\
         0905111001,,,Gội đầu 10 buổi,1500000,{start},,,Thư Giãn,5,0,5,,,\n\
         0905222002,Trần Thị Lan,female,Combo,2000000,{start},,,Thư Giãn,5,0,3,,,\n"
    );
    let report = run(&h, &app, &file).await?;
    assert_eq!(
        (report.imported, report.payments_booked),
        (2, 1),
        "{report:?}"
    );
    let sales: Vec<(chrono::NaiveDate, String)> =
        sqlx::query_as("SELECT business_date, source FROM app.sale")
            .fetch_all(&h.owner)
            .await?;
    assert_eq!(sales, [(paid_on, "paper_book".to_owned())]);

    // The paper date's dashboard, ledger and range report carry the money.
    let daily = call(
        &h.app,
        "GET",
        &format!("/reports/daily?date={paid_on}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(
        daily.body["revenue"]["collected"],
        json!({"cash": 0, "bank_transfer": 1_500_000, "total": 1_500_000})
    );
    assert_eq!(
        daily.body["revenue"]["collected_by_kind"]["package"],
        1_500_000
    );
    assert_eq!(daily.body["counts"]["packages_sold"], 1);
    let ledger = call(
        &h.app,
        "GET",
        &format!("/reports/ledger?from={paid_on}&to={paid_on}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(ledger.body["rows"][0]["entry"], "sale");
    assert_eq!(ledger.body["rows"][0]["package"], "Gội đầu 10 buổi");
    assert_eq!(ledger.body["rows"][0]["bank_transfer"], 1_500_000);
    assert_eq!(ledger.body["days"][0]["total"], 1_500_000);
    let range = call(
        &h.app,
        "GET",
        &format!("/reports/range?from={start}&to={later}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(range.body["revenue"]["net"]["total"], 1_500_000);
    let today_report = call(&h.app, "GET", "/reports/daily", Some(&owner), None).await;
    assert_eq!(
        today_report.body["revenue"]["collected"]["total"], 0,
        "not on the import day"
    );

    // "Đã chi" and the history include it; nothing in the API tells it apart.
    let ha: String =
        sqlx::query_scalar("SELECT id::text FROM app.customer WHERE phone_e164 = '+84905111001'")
            .fetch_one(&h.owner)
            .await?;
    let summary = call(
        &h.app,
        "GET",
        &format!("/customers/{ha}/summary"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(
        (
            summary.body["paid"].clone(),
            summary.body["net_spent"].clone()
        ),
        (json!(1_500_000), json!(1_500_000))
    );
    let history = call(
        &h.app,
        "GET",
        &format!("/customers/{ha}/history"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(history.body["events"][0]["kind"], "purchase");
    assert_eq!(history.body["events"][0]["business_date"], json!(paid_on));
    let packages = call(
        &h.app,
        "GET",
        &format!("/customers/{ha}/packages"),
        Some(&owner),
        None,
    )
    .await;
    let sale_id = packages.body[0]["sale_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!sale_id.is_empty(), "{}", packages.body);
    let sale = call(
        &h.app,
        "GET",
        &format!("/sales/{sale_id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(sale.status, axum::http::StatusCode::OK);
    for body in [&packages.body, &sale.body, &ledger.body, &history.body] {
        assert!(
            !body.to_string().contains("paper_book"),
            "never shown: {body}"
        );
    }
    assert_eq!(
        count(
            &h.owner,
            "SELECT count(*) FROM app.audit_log WHERE action = 'sale.paper_book' AND after ->> 'source' = 'paper_book'"
        )
        .await?,
        1
    );

    // Re-running the same file books nothing again.
    let again = run(&h, &app, &file).await?;
    assert_eq!(
        (
            again.imported,
            again.skipped_existing,
            again.payments_booked
        ),
        (0, 2, 0)
    );
    assert_eq!(count(&h.owner, "SELECT count(*) FROM app.sale").await?, 1);

    // Lan's package was imported without money; a later file adds it once.
    let add_paid = format!(
        "{PAID_HEADER}\n0905222002,,,Combo,2000000,{start},,,Thư Giãn,5,0,3,800000,{later},\n"
    );
    let added = run(&h, &app, &add_paid).await?;
    assert_eq!((added.imported, added.payments_booked), (0, 1));
    let twice = run(&h, &app, &add_paid).await?;
    assert_eq!(twice.payments_booked, 0);
    assert_eq!(count(&h.owner, "SELECT count(*) FROM app.sale").await?, 2);
    assert_eq!(
        count(
            &h.owner,
            &format!("SELECT count(*) FROM app.payment WHERE method = 'cash' AND business_date = '{later}'")
        )
        .await?,
        1,
        "method defaults to cash"
    );

    // Voiding a paper sale reverses its money (today) and leaves the paper package alone.
    let void = common::call_with_headers(
        &h.app,
        "POST",
        &format!("/sales/{sale_id}/void"),
        Some(&owner),
        Some(json!({"reason": "Nhập nhầm số tiền"})),
        &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
    )
    .await;
    assert_eq!(void.status, axum::http::StatusCode::OK, "{}", void.body);
    let after = call(
        &h.app,
        "GET",
        &format!("/customers/{ha}/packages"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(after.body[0]["status"], "active");

    // The database refuses to tie a paper-book package to an app sale or to relink it.
    let relink = sqlx::query(
        "UPDATE app.customer_package SET sale_id = NULL WHERE sale_id IS NOT NULL AND origin = 'opening_balance'",
    )
    .execute(&h.owner)
    .await;
    assert!(relink.is_err(), "the link is final");
    Ok(())
}

/// The same fields on the owner-only HTTP route, with their validation; back-dated money is
/// allowed (the owner may do everything), the future is not.
#[sqlx::test(migrator = "kernel::MIGRATOR")]
async fn the_import_route_books_paper_money_and_validates_it(pool: PgPool) -> TestResult {
    let h = harness(pool).await?;
    seed_menu(&h.owner, "vpyd").await?;
    let owner = session_for(&h, OWNER_PHONE).await;
    let service: String = sqlx::query_scalar("SELECT id::text FROM app.service LIMIT 1")
        .fetch_one(&h.owner)
        .await?;
    let body = |paid: Value| {
        let mut package = json!({"name": "Gói sổ giấy", "sold_price": 1_000_000,
            "starts_on": days_ago(60),
            "items": [{"service_id": service, "sessions_purchased": 10, "sessions_remaining": 4}]});
        if let (Some(p), Some(extra)) = (package.as_object_mut(), paid.as_object()) {
            p.extend(extra.clone());
        }
        json!({"customer": {"full_name": "Khách sổ giấy", "phone": "0905333003"},
               "packages": [package]})
    };
    let post = |b: Value| {
        let app = h.app.clone();
        let owner = owner.clone();
        async move {
            common::call_with_headers(
                &app,
                "POST",
                "/opening-balances",
                Some(&owner),
                Some(b),
                &[("Idempotency-Key", &uuid::Uuid::now_v7().to_string())],
            )
            .await
        }
    };
    for (paid, field, code) in [
        (
            json!({"paid_amount": 500_000}),
            "packages[0].paid_on",
            "required",
        ),
        (
            json!({"paid_amount": 500_000, "paid_on": days_ago(-1)}),
            "packages[0].paid_on",
            "in_future",
        ),
        (
            json!({"paid_amount": -1, "paid_on": days_ago(1)}),
            "packages[0].paid_amount",
            "negative",
        ),
        (
            json!({"paid_amount": 10_000_000_001_i64, "paid_on": days_ago(1)}),
            "packages[0].paid_amount",
            "out_of_range",
        ),
        (
            json!({"paid_on": days_ago(1)}),
            "packages[0].paid_on",
            "not_allowed",
        ),
    ] {
        let r = post(body(paid.clone())).await;
        assert_eq!(
            r.status,
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "{paid}: {}",
            r.body
        );
        assert_eq!(
            (
                r.body["errors"][0]["field"].clone(),
                r.body["errors"][0]["code"].clone()
            ),
            (json!(field), json!(code)),
            "{paid}"
        );
    }
    let bad_method = post(body(
        json!({"paid_amount": 1, "paid_on": days_ago(1), "method": "card"}),
    ))
    .await;
    assert_eq!(
        bad_method.status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        count(&h.owner, "SELECT count(*) FROM app.customer").await?,
        0,
        "nothing written"
    );

    let paid_on = days_ago(400);
    let ok = post(body(
        json!({"paid_amount": 900_000, "paid_on": paid_on, "method": "bank_transfer"}),
    ))
    .await;
    assert_eq!(ok.status, axum::http::StatusCode::CREATED, "{}", ok.body);
    assert!(ok.body["customer_packages"][0]["sale_id"].is_string());
    let daily = call(
        &h.app,
        "GET",
        &format!("/reports/daily?date={paid_on}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(daily.body["revenue"]["net"]["bank_transfer"], 900_000);
    Ok(())
}
