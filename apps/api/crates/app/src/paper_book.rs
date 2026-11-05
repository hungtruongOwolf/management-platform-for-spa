//! `app import-paper-book --tenant <slug> --file <csv> [--dry-run]`: the one-off migration of the
//! paper "sổ theo dõi liệu trình" into a spa (spec management-views §7). Operator tool only: the
//! team runs it after checking the CSV; no HTTP route reaches it.
//!
//! One CSV row = one service of one package; rows with the same `phone` + `package_name` +
//! `starts_on` form one package. Every package goes through `packages::opening_balance::import`
//! acting as the spa's owner, so validation, audit and "no sale, no payment" hold exactly as for
//! the front desk. A package the customer already has (same name and start day, no sale) is
//! skipped, so running the same file twice imports nothing new.

use crate::demo::{member_actor, owner_of};
use anyhow::{Context, bail};
use chrono::NaiveDate;
use kernel::crypto::FieldCipher;
use kernel::idempotency::hash_request;
use kernel::{PhoneE164, TenantId};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Columns of the CSV, in any order (header row required).
pub const COLUMNS: [&str; 12] = [
    "phone",
    "full_name",
    "gender",
    "package_name",
    "sold_price",
    "starts_on",
    "expires_on",
    "gift_note",
    "service",
    "sessions_purchased",
    "sessions_bonus",
    "sessions_remaining",
];
const REQUIRED: [&str; 7] = [
    "phone",
    "package_name",
    "sold_price",
    "starts_on",
    "service",
    "sessions_purchased",
    "sessions_remaining",
];
/// `POST /opening-balances` takes at most this many packages per customer and call.
const PACKAGES_PER_CALL: usize = packages::opening_balance::MAX_PACKAGES;

pub struct ImportOptions<'a> {
    pub tenant_slug: &'a str,
    pub csv: &'a str,
    /// Parse, resolve customers and services, report; write nothing.
    pub dry_run: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub rows: usize,
    pub packages: usize,
    pub customers: usize,
    pub customers_created: usize,
    pub imported: usize,
    pub skipped_existing: usize,
}

/// Splits CSV text into records (RFC 4180: quoted fields, `""` escapes, commas and line breaks
/// inside quotes, CRLF). Returns `(line number of the record start, fields)`.
pub fn parse_csv(text: &str) -> anyhow::Result<Vec<(usize, Vec<String>)>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = Vec::new();
    let (mut field, mut fields) = (String::new(), Vec::new());
    let (mut line, mut start, mut quoted) = (1, 1, false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, other) => {
                if other == '\n' {
                    line += 1;
                }
                field.push(other);
            }
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => fields.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                fields.push(std::mem::take(&mut field));
                if fields.iter().any(|f| !f.trim().is_empty()) {
                    records.push((start, std::mem::take(&mut fields)));
                }
                fields.clear();
                line += 1;
                start = line;
            }
            (false, other) => field.push(other),
        }
    }
    if quoted {
        bail!("line {start}: unterminated quoted field");
    }
    fields.push(field);
    if fields.iter().any(|f| !f.trim().is_empty()) {
        records.push((start, fields));
    }
    Ok(records)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    service: String,
    purchased: u32,
    bonus: u32,
    remaining: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Package {
    line: usize,
    phone: PhoneE164,
    full_name: Option<String>,
    gender: Option<String>,
    name: String,
    sold_price: i64,
    starts_on: NaiveDate,
    expires_on: Option<NaiveDate>,
    gift_note: Option<String>,
    items: Vec<Item>,
}

fn optional(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_owned())
}

/// Parses and groups the rows into packages, in file order. Every bad row is reported at once.
fn packages_of(csv: &str) -> anyhow::Result<(usize, Vec<Package>)> {
    let records = parse_csv(csv)?;
    let Some(((_, header), rows)) = records.split_first() else {
        bail!(
            "the file is empty (expected a header row: {})",
            COLUMNS.join(",")
        );
    };
    let index: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_lowercase(), i))
        .collect();
    let missing: Vec<&str> = REQUIRED
        .iter()
        .copied()
        .filter(|c| !index.contains_key(*c))
        .collect();
    if !missing.is_empty() {
        bail!("missing column(s): {}", missing.join(", "));
    }
    if let Some(unknown) = index.keys().find(|k| !COLUMNS.contains(&k.as_str())) {
        bail!(
            "unknown column {unknown:?} (expected: {})",
            COLUMNS.join(",")
        );
    }
    let mut errors = Vec::new();
    let mut packages: Vec<Package> = Vec::new();
    for (line, fields) in rows {
        let get = |c: &str| {
            index
                .get(c)
                .and_then(|i| fields.get(*i))
                .map_or("", |v| v.trim())
        };
        let mut problem = |what: String| errors.push(format!("line {line}: {what}"));
        for c in REQUIRED {
            if get(c).is_empty() {
                problem(format!("{c} is required"));
            }
        }
        let phone = PhoneE164::parse(get("phone"));
        if phone.is_err() {
            problem(format!(
                "phone {:?} is not a Vietnamese number",
                get("phone")
            ));
        }
        let number = |c: &str| get(c).parse::<u32>().ok();
        let date = |c: &str| NaiveDate::parse_from_str(get(c), "%Y-%m-%d").ok();
        let sold_price = get("sold_price").parse::<i64>().ok();
        let (purchased, remaining) = (number("sessions_purchased"), number("sessions_remaining"));
        let bonus = if get("sessions_bonus").is_empty() {
            Some(0)
        } else {
            number("sessions_bonus")
        };
        let starts_on = date("starts_on");
        let expires_on = date("expires_on");
        if sold_price.is_none() {
            problem(format!(
                "sold_price {:?} is not a whole number of đồng",
                get("sold_price")
            ));
        }
        for (c, v) in [
            ("sessions_purchased", purchased),
            ("sessions_bonus", bonus),
            ("sessions_remaining", remaining),
        ] {
            if v.is_none() && !get(c).is_empty() {
                problem(format!("{c} {:?} is not a whole number", get(c)));
            }
        }
        if starts_on.is_none() && !get("starts_on").is_empty() {
            problem("starts_on must be YYYY-MM-DD".into());
        }
        if expires_on.is_none() && !get("expires_on").is_empty() {
            problem("expires_on must be YYYY-MM-DD".into());
        }
        let gender = optional(get("gender"));
        if gender
            .as_deref()
            .is_some_and(|g| g != "female" && g != "male")
        {
            problem("gender must be female, male or empty".into());
        }
        let (
            Ok(phone),
            Some(sold_price),
            Some(starts_on),
            Some(purchased),
            Some(bonus),
            Some(remaining),
        ) = (phone, sold_price, starts_on, purchased, bonus, remaining)
        else {
            continue;
        };
        let item = Item {
            service: get("service").to_owned(),
            purchased,
            bonus,
            remaining,
        };
        let name = get("package_name").to_owned();
        match packages
            .iter_mut()
            .find(|p| p.phone == phone && p.name == name && p.starts_on == starts_on)
        {
            Some(p) => p.items.push(item),
            None => packages.push(Package {
                line: *line,
                phone,
                full_name: optional(get("full_name")),
                gender,
                name,
                sold_price,
                starts_on,
                expires_on,
                gift_note: optional(get("gift_note")),
                items: vec![item],
            }),
        }
    }
    if !errors.is_empty() {
        bail!(
            "the file has errors, nothing was imported:\n{}",
            errors.join("\n")
        );
    }
    Ok((rows.len(), packages))
}

fn package_json(p: &Package, services: &HashMap<String, Uuid>) -> anyhow::Result<Value> {
    let items = p
        .items
        .iter()
        .map(|i| {
            let id = services.get(&i.service.to_lowercase()).with_context(|| {
                format!(
                    "line {}: no service named {:?} in the menu",
                    p.line, i.service
                )
            })?;
            Ok(json!({"service_id": id, "sessions_purchased": i.purchased,
                      "sessions_bonus": i.bonus, "sessions_remaining": i.remaining}))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(
        json!({"name": p.name, "sold_price": p.sold_price, "starts_on": p.starts_on,
              "expires_on": p.expires_on, "gift_note": p.gift_note, "items": items}),
    )
}

/// Imports `options.csv` into the spa. `owner` is the schema-owner pool (lookups, as the other
/// CLI commands); `app` is the `app_rw` pool the use case runs on, with RLS.
pub async fn import(
    owner: &PgPool,
    app: &PgPool,
    cipher: &FieldCipher,
    options: &ImportOptions<'_>,
) -> anyhow::Result<ImportReport> {
    let (rows, packages) = packages_of(options.csv)?;
    let slug = options.tenant_slug;
    let tenant = TenantId::from_uuid(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM app.tenant WHERE slug = $1")
            .bind(slug)
            .fetch_optional(owner)
            .await?
            .with_context(|| format!("no spa with slug {slug:?}"))?,
    );
    let actor = member_actor(app, owner_of(owner, tenant).await?, tenant).await?;
    let services: HashMap<String, Uuid> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, name FROM app.service WHERE tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .fetch_all(owner)
    .await?
    .into_iter()
    .map(|(id, name)| (name.trim().to_lowercase(), id))
    .collect();
    // Resolve every service first: a typo anywhere stops the run before anything is written.
    let bodies = packages
        .iter()
        .map(|p| package_json(p, &services))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut report = ImportReport {
        rows,
        packages: packages.len(),
        ..ImportReport::default()
    };
    let mut phones: Vec<&PhoneE164> = Vec::new();
    for p in &packages {
        if !phones.contains(&&p.phone) {
            phones.push(&p.phone);
        }
    }
    report.customers = phones.len();
    for phone in phones {
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM app.customer
             WHERE tenant_id = $1 AND phone_e164 = $2 AND anonymized_at IS NULL",
        )
        .bind(tenant.as_uuid())
        .bind(phone.as_str())
        .fetch_optional(owner)
        .await?;
        let mut todo = Vec::new();
        for (p, body) in packages
            .iter()
            .zip(&bodies)
            .filter(|(p, _)| &p.phone == phone)
        {
            let already = match existing {
                Some(customer) => {
                    sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS (SELECT 1 FROM app.customer_package
                     WHERE tenant_id = $1 AND customer_id = $2 AND sale_id IS NULL
                       AND name_snapshot = $3 AND starts_on = $4)",
                    )
                    .bind(tenant.as_uuid())
                    .bind(customer)
                    .bind(p.name.trim())
                    .bind(p.starts_on)
                    .fetch_one(owner)
                    .await?
                }
                None => false,
            };
            if already {
                report.skipped_existing += 1;
            } else {
                todo.push((p, body.clone()));
            }
        }
        if todo.is_empty() {
            continue;
        }
        let first = todo[0].0;
        if existing.is_none() && first.full_name.is_none() {
            bail!(
                "line {}: full_name is required for a new customer ({})",
                first.line,
                phone.as_str()
            );
        }
        if options.dry_run {
            report.imported += todo.len();
            report.customers_created += usize::from(existing.is_none());
            continue;
        }
        let mut customer = existing;
        for chunk in todo.chunks(PACKAGES_PER_CALL) {
            let mut body =
                json!({"packages": chunk.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>()});
            match customer {
                Some(id) => body["customer_id"] = json!(id),
                None => {
                    body["customer"] = json!({"full_name": first.full_name, "phone": phone.as_str(),
                                              "gender": first.gender});
                }
            }
            let input = serde_json::from_value(body).context("import body")?;
            let key = Uuid::now_v7();
            let (_, created) = packages::opening_balance::import(
                app,
                cipher,
                &actor,
                key,
                hash_request(key.as_bytes()),
                &input,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("line {}: import refused: {} ({e:?})", first.line, e.code())
            })?;
            if customer.is_none() {
                report.customers_created += 1;
                customer = created["customer_id"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok());
            }
            report.imported += chunk.len();
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "phone,full_name,gender,package_name,sold_price,starts_on,expires_on,gift_note,service,sessions_purchased,sessions_bonus,sessions_remaining";

    #[test]
    fn csv_handles_quotes_commas_crlf_and_bom() -> anyhow::Result<()> {
        let text = "\u{feff}a,b\r\n\"x, y\",\"say \"\"hi\"\"\"\r\n\r\n\"multi\nline\",z";
        let records = parse_csv(text)?;
        assert_eq!(
            records,
            [
                (1, vec!["a".to_owned(), "b".to_owned()]),
                (2, vec!["x, y".to_owned(), "say \"hi\"".to_owned()]),
                (4, vec!["multi\nline".to_owned(), "z".to_owned()]),
            ]
        );
        assert!(parse_csv("a,\"b").is_err());
        Ok(())
    }

    #[test]
    fn rows_group_into_packages_by_phone_name_and_start() -> anyhow::Result<()> {
        let csv = format!(
            "{HEADER}\n\
             0905111001,Hà,female,Gói A,1500000,2026-06-01,,Khăn,Gội,10,2,4\n\
             0905 111 001,,,Gói A,1500000,2026-06-01,,,Massage,5,0,5\n\
             0905111001,,,Gói A,900000,2026-01-01,2026-12-31,,Gội,5,,1\n\
             0905111002,Lan,,Gói B,0,2026-05-01,,,Gội,3,0,0\n"
        );
        let (rows, packages) = packages_of(&csv)?;
        assert_eq!((rows, packages.len()), (4, 3));
        assert_eq!(
            packages[0].items.len(),
            2,
            "same phone (any format) + name + start"
        );
        assert_eq!(packages[0].gift_note.as_deref(), Some("Khăn"));
        assert_eq!(
            packages[1].expires_on,
            NaiveDate::from_ymd_opt(2026, 12, 31)
        );
        assert_eq!(packages[1].items[0].bonus, 0, "empty bonus = 0");
        Ok(())
    }

    #[test]
    fn every_bad_row_is_reported_with_its_line() {
        let csv = format!(
            "{HEADER}\n\
             12345,Hà,x,Gói A,abc,01/06/2026,,,Gội,10,0,4\n\
             0905111001,Hà,,,1000,2026-06-01,,,Gội,10,0,\n"
        );
        let message = packages_of(&csv)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        for expected in [
            "line 2: phone",
            "line 2: sold_price",
            "line 2: starts_on must be YYYY-MM-DD",
            "line 2: gender",
            "line 3: package_name is required",
            "line 3: sessions_remaining is required",
        ] {
            assert!(message.contains(expected), "{expected} in {message}");
        }
        assert!(packages_of("phone,name\n").is_err(), "missing columns");
        assert!(
            packages_of(&format!("{HEADER},extra\n")).is_err(),
            "unknown column"
        );
    }
}
