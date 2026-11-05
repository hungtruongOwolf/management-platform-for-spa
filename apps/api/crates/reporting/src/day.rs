//! Business-day reports: the daily dashboard ("Hôm nay"), the date-range report and the revenue
//! book. Counts come from `visit` / `visit_line` / `sale`; money from `payment`, `sale_line` and
//! `refund`, the same rows the `app.revenue_book` view (0013) lists line by line (tests reconcile
//! the two). Days are `business_date` values the writers computed in the spa's time zone.

use crate::cursor;
use crate::dto::{
    DailyReport, DayComparison, DayCounts, DayHeadline, Granularity, LedgerDay, LedgerEntry,
    LedgerPage, LedgerRow, RangeBucket, RangePreset, RangeQuery, RangeReport, Revenue, SaleKind,
    TechnicianDay, TopPackage, TopService,
};
use chrono::{Datelike, Days, Months, NaiveDate};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use sqlx::PgPool;
use std::collections::HashMap;

/// Longest `GET /reports/range` window by day or week: a full (leap) year.
pub const MAX_RANGE_DAYS: u64 = 366;
/// Longest window by month or year: five (leap) years. Few buckets, one range scan.
pub const MAX_LONG_RANGE_DAYS: u64 = 5 * 366;
/// Longest `GET /reports/ledger` window: a quarter.
pub const MAX_LEDGER_DAYS: u64 = 92;
const TOP: i64 = 10;

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

fn minus_days(d: NaiveDate, n: u64) -> Result<NaiveDate, AppError> {
    d.checked_sub_days(Days::new(n))
        .ok_or_else(|| field("date", "out_of_range"))
}

/// First day of the bucket `d` falls in; must match Postgres `date_trunc` (ISO weeks start Monday).
fn bucket_of(d: NaiveDate, g: Granularity) -> NaiveDate {
    match g {
        Granularity::Day => d,
        Granularity::Week => d - Days::new(u64::from(d.weekday().num_days_from_monday())),
        Granularity::Month => d.with_day(1).unwrap_or(d),
        Granularity::Year => d.with_ordinal(1).unwrap_or(d),
    }
}

const fn max_days(g: Granularity) -> u64 {
    match g {
        Granularity::Day | Granularity::Week => MAX_RANGE_DAYS,
        Granularity::Month | Granularity::Year => MAX_LONG_RANGE_DAYS,
    }
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// `[from, to]` and the default granularity of a preset, relative to the spa's `today`.
fn preset_window(
    preset: RangePreset,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate, Granularity), AppError> {
    let bad = || field("preset", "out_of_range");
    let months_back = |n: u32| {
        first_of_month(today)
            .checked_sub_months(Months::new(n))
            .ok_or_else(bad)
    };
    let jan_first = |year: i32| NaiveDate::from_ymd_opt(year, 1, 1).ok_or_else(bad);
    Ok(match preset {
        RangePreset::Last7Days => (minus_days(today, 6)?, today, Granularity::Day),
        RangePreset::Last30Days => (minus_days(today, 29)?, today, Granularity::Day),
        RangePreset::ThisMonth => (first_of_month(today), today, Granularity::Day),
        RangePreset::LastMonth => {
            let start = months_back(1)?;
            let end = first_of_month(today).pred_opt().ok_or_else(bad)?;
            (start, end, Granularity::Day)
        }
        RangePreset::ThisYear => (jan_first(today.year())?, today, Granularity::Month),
        RangePreset::Last12Months => (months_back(11)?, today, Granularity::Month),
        RangePreset::Last5Years => (jan_first(today.year() - 4)?, today, Granularity::Year),
    })
}

/// Every day from `from` to `to` inclusive.
fn days(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    from.iter_days().take_while(|d| *d <= to).collect()
}

/// Accumulates counts and money per bucket (bucket = first day as `date_trunc` returns it).
#[derive(Default)]
struct Books {
    counts: HashMap<NaiveDate, DayCounts>,
    revenue: HashMap<NaiveDate, Revenue>,
}

impl Books {
    /// Reads the books of `from..=to` (only the days in `only`, when given), grouped by
    /// `granularity`. Money only when `with_money`. A range is a range scan on the day indexes,
    /// so a five-year report never ships 1 800 dates to the database.
    async fn read(
        tx: &mut TenantTx,
        (from, to): (NaiveDate, NaiveDate),
        only: Option<&[NaiveDate]>,
        granularity: Granularity,
        with_money: bool,
    ) -> Result<Self, AppError> {
        let tenant = tx.tenant().as_uuid();
        let unit = granularity.as_sql();
        let mut books = Self::default();

        for r in sqlx::query!(
            r#"SELECT date_trunc($3, v.business_date::timestamp)::date AS "bucket!",
                      count(*) FILTER (WHERE v.status = 'completed') AS "visits!",
                      count(DISTINCT v.customer_id) FILTER (WHERE v.status = 'completed') AS "customers!",
                      count(*) FILTER (WHERE v.status = 'voided') AS "voided!"
               FROM app.visit v
               WHERE v.tenant_id = $1 AND v.business_date BETWEEN $2 AND $4 AND ($5::date[] IS NULL OR v.business_date = ANY($5))
               GROUP BY 1"#,
            tenant,
            from,
            unit,
            to,
            only
        )
        .fetch_all(tx.conn())
        .await?
        {
            let c = books.counts.entry(r.bucket).or_default();
            c.visits = r.visits;
            c.customers = r.customers;
            c.voided_visits = r.voided;
        }

        for r in sqlx::query!(
            r#"SELECT date_trunc($3, v.business_date::timestamp)::date AS "bucket!",
                      coalesce(sum(l.qty) FILTER (WHERE l.source = 'package'), 0)::bigint AS "sessions!",
                      coalesce(sum(l.qty) FILTER (WHERE l.source = 'one_off'), 0)::bigint AS "one_off!"
               FROM app.visit v
               JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id
               WHERE v.tenant_id = $1 AND v.business_date BETWEEN $2 AND $4 AND ($5::date[] IS NULL OR v.business_date = ANY($5)) AND v.status = 'completed'
               GROUP BY 1"#,
            tenant,
            from,
            unit,
            to,
            only
        )
        .fetch_all(tx.conn())
        .await?
        {
            let c = books.counts.entry(r.bucket).or_default();
            c.sessions_used = r.sessions;
            c.one_off_services = r.one_off;
        }

        for r in sqlx::query!(
            r#"SELECT date_trunc($3, s.business_date::timestamp)::date AS "bucket!",
                      count(*) AS "sold!"
               FROM app.sale s
               JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
               WHERE s.tenant_id = $1 AND s.business_date BETWEEN $2 AND $4 AND ($5::date[] IS NULL OR s.business_date = ANY($5)) AND s.status = 'open'
               GROUP BY 1"#,
            tenant,
            from,
            unit,
            to,
            only
        )
        .fetch_all(tx.conn())
        .await?
        {
            books.counts.entry(r.bucket).or_default().packages_sold = r.sold;
        }

        if with_money {
            // Same books as `app.revenue_book`, aggregated without its per-sale descriptions
            // (a year of sales would pay for thousands of string_agg it then throws away).
            // A sale is a package sale when it has a package line; refunds are always packages.
            for r in sqlx::query!(
                r#"SELECT date_trunc($3, p.business_date::timestamp)::date AS "bucket!",
                          p.kind AS "entry!", p.method AS "method!",
                          CASE WHEN pk.sale_id IS NULL THEN 'one_off' ELSE 'package' END AS "sale_kind!",
                          sum(p.amount)::bigint AS "amount!"
                   FROM app.payment p
                   LEFT JOIN (SELECT DISTINCT sl.sale_id FROM app.sale_line sl
                              WHERE sl.tenant_id = $1 AND sl.line_type = 'package') pk
                     ON pk.sale_id = p.sale_id
                   WHERE p.tenant_id = $1 AND p.business_date BETWEEN $2 AND $4 AND ($5::date[] IS NULL OR p.business_date = ANY($5))
                   GROUP BY 1, 2, 3, 4
                   UNION ALL
                   SELECT date_trunc($3, r.business_date::timestamp)::date, 'refund', r.method,
                          'package', sum(r.amount)::bigint
                   FROM app.refund r
                   WHERE r.tenant_id = $1 AND r.business_date BETWEEN $2 AND $4 AND ($5::date[] IS NULL OR r.business_date = ANY($5))
                   GROUP BY 1, 2, 3, 4"#,
                tenant,
                from,
                unit,
                to,
                only
            )
            .fetch_all(tx.conn())
            .await?
            {
                let money = books.revenue.entry(r.bucket).or_default();
                let kind = SaleKind::parse(&r.sale_kind);
                let (target, by_kind) = match r.entry.as_str() {
                    "payment" => (&mut money.collected, Some(&mut money.collected_by_kind)),
                    "reversal" => (&mut money.voids, Some(&mut money.voids_by_kind)),
                    _ => (&mut money.refunds, None),
                };
                target.add(&r.method, r.amount);
                if let Some(by_kind) = by_kind {
                    by_kind.add(kind, r.amount);
                }
            }
        }
        Ok(books)
    }

    fn counts(&self, bucket: NaiveDate) -> DayCounts {
        self.counts.get(&bucket).copied().unwrap_or_default()
    }

    fn revenue(&self, bucket: NaiveDate) -> Revenue {
        self.revenue
            .get(&bucket)
            .copied()
            .unwrap_or_default()
            .settle()
    }
}

fn headline(books: &Books, date: NaiveDate, with_money: bool) -> DayHeadline {
    let c = books.counts(date);
    DayHeadline {
        date,
        customers: c.customers,
        visits: c.visits,
        sessions_used: c.sessions_used,
        packages_sold: c.packages_sold,
        net_revenue: with_money.then(|| books.revenue(date).net.total),
    }
}

async fn today(tx: &mut TenantTx) -> Result<NaiveDate, AppError> {
    Ok(kernel::tenant::profile(tx).await?.business_date.as_naive())
}

/// `GET /reports/daily?date=` (`report.read`; money with `report.revenue.read`).
pub async fn daily(
    pool: &PgPool,
    actor: &Actor,
    date: Option<NaiveDate>,
) -> Result<DailyReport, AppError> {
    actor.require(Permission::ReportRead)?;
    let with_money = actor.can(Permission::ReportRevenueRead);
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let date = match date {
        Some(d) => d,
        None => today(&mut tx).await?,
    };
    let (yesterday, last_week) = (minus_days(date, 1)?, minus_days(date, 7)?);
    let books = Books::read(
        &mut tx,
        (last_week, date),
        Some(&[date, yesterday, last_week]),
        Granularity::Day,
        with_money,
    )
    .await?;
    let tenant = tx.tenant().as_uuid();
    let technicians = sqlx::query_as!(
        TechnicianDay,
        r#"SELECT l.staff_id, st.full_name AS name,
                  count(DISTINCT v.id) AS "visits!",
                  coalesce(sum(l.qty) FILTER (WHERE l.source = 'package'), 0)::bigint AS "sessions_used!",
                  coalesce(sum(l.qty) FILTER (WHERE l.source = 'one_off'), 0)::bigint AS "one_off_services!"
           FROM app.visit v
           JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id
           JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id
           WHERE v.tenant_id = $1 AND v.business_date = $2 AND v.status = 'completed'
           GROUP BY l.staff_id, st.full_name
           ORDER BY 3 DESC, st.full_name, l.staff_id"#,
        tenant,
        date
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(DailyReport {
        date,
        counts: books.counts(date),
        revenue: with_money.then(|| books.revenue(date)),
        technicians,
        compare: DayComparison {
            yesterday: headline(&books, yesterday, with_money),
            same_weekday_last_week: headline(&books, last_week, with_money),
        },
    })
}

/// Validated `[from, to]` with defaults: `to` = today, `from` = `to` − (default_days − 1).
fn window(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    today: NaiveDate,
    default_days: u64,
    max_days: u64,
) -> Result<(NaiveDate, NaiveDate), AppError> {
    let to = to.unwrap_or_else(|| from.map_or(today, |f| f.max(today)));
    let from = match from {
        Some(f) => f,
        None => minus_days(to, default_days - 1)?,
    };
    if from > to {
        return Err(field("from", "after_to"));
    }
    let span = u64::try_from((to - from).num_days()).unwrap_or(u64::MAX);
    if span >= max_days {
        return Err(field("to", "range_too_long"));
    }
    Ok((from, to))
}

/// The period and granularity a range query asks for: a preset (its default granularity unless
/// one is given) or `from` / `to` (default the last 7 days, by day). Bounded per granularity.
fn range_window(
    query: &RangeQuery,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate, Granularity), AppError> {
    let (from, to, granularity) = match query.preset {
        Some(_) if query.from.is_some() || query.to.is_some() => {
            return Err(field("preset", "conflicts_with_range"));
        }
        Some(preset) => {
            let (from, to, default) = preset_window(preset, today)?;
            (Some(from), Some(to), query.granularity.unwrap_or(default))
        }
        None => (query.from, query.to, query.granularity.unwrap_or_default()),
    };
    let (from, to) = window(from, to, today, 7, max_days(granularity))?;
    Ok((from, to, granularity))
}

/// `GET /reports/range?from=&to=&granularity=&preset=` (`report.read`; money with
/// `report.revenue.read`).
pub async fn range(
    pool: &PgPool,
    actor: &Actor,
    query: &RangeQuery,
) -> Result<RangeReport, AppError> {
    actor.require(Permission::ReportRead)?;
    let with_money = actor.can(Permission::ReportRevenueRead);
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = today(&mut tx).await?;
    let (from, to, granularity) = range_window(query, today)?;
    let all_days = days(from, to);
    let books = Books::read(&mut tx, (from, to), None, granularity, with_money).await?;
    let tenant = tx.tenant().as_uuid();

    let customers = sqlx::query_scalar!(
        r#"SELECT count(DISTINCT customer_id) AS "n!" FROM app.visit
           WHERE tenant_id = $1 AND business_date BETWEEN $2 AND $3 AND status = 'completed'"#,
        tenant,
        from,
        to
    )
    .fetch_one(tx.conn())
    .await?;

    let top_services = sqlx::query!(
        r#"SELECT l.service_id, s.name, sc.name AS category_name,
                  coalesce(sum(l.qty) FILTER (WHERE l.source = 'package'), 0)::bigint AS "sessions!",
                  coalesce(sum(l.qty) FILTER (WHERE l.source = 'one_off'), 0)::bigint AS "one_off!",
                  coalesce(sum(l.line_value) FILTER (WHERE l.source = 'one_off'), 0)::bigint AS "one_off_revenue!"
           FROM app.visit v
           JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id
           JOIN app.service s ON s.tenant_id = l.tenant_id AND s.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = s.tenant_id AND sc.id = s.category_id
           WHERE v.tenant_id = $1 AND v.business_date BETWEEN $2 AND $3 AND v.status = 'completed'
           GROUP BY l.service_id, s.name, sc.name
           ORDER BY sum(l.qty) DESC, s.name, l.service_id
           LIMIT $4"#,
        tenant,
        from,
        to,
        TOP
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|r| TopService {
        service_id: r.service_id,
        name: r.name,
        category_name: r.category_name,
        sessions_used: r.sessions,
        one_off_services: r.one_off,
        one_off_revenue: with_money.then_some(r.one_off_revenue),
    })
    .collect();

    let top_packages = sqlx::query!(
        r#"SELECT cp.plan_id, min(cp.name_snapshot) AS "name!", count(*) AS "sold!",
                  coalesce(sum(cp.sold_price), 0)::bigint AS "revenue!"
           FROM app.sale s
           JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
           WHERE s.tenant_id = $1 AND s.business_date BETWEEN $2 AND $3 AND s.status = 'open'
           GROUP BY cp.plan_id
           ORDER BY 3 DESC, 2, cp.plan_id
           LIMIT $4"#,
        tenant,
        from,
        to,
        TOP
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|r| TopPackage {
        plan_id: r.plan_id,
        name: r.name,
        sold: r.sold,
        revenue: with_money.then_some(r.revenue),
    })
    .collect();
    tx.commit().await?;

    let mut buckets: Vec<NaiveDate> = all_days
        .iter()
        .map(|d| bucket_of(*d, granularity))
        .collect();
    buckets.dedup();
    let mut totals = DayCounts::default();
    let mut total_money = Revenue::default();
    let series: Vec<RangeBucket> = buckets
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let end = buckets
                .get(i + 1)
                .and_then(|next| next.pred_opt())
                .map_or(to, |e| e.min(to));
            let counts = books.counts(*key);
            let revenue = books.revenue(*key);
            totals = totals.plus(counts);
            total_money = total_money.plus(revenue);
            RangeBucket {
                start: (*key).max(from),
                end,
                counts,
                revenue: with_money.then_some(revenue),
                net_revenue: with_money.then_some(revenue.net.total),
            }
        })
        .collect();
    totals.customers = customers;
    Ok(RangeReport {
        from,
        to,
        granularity,
        preset: query.preset,
        totals,
        revenue: with_money.then_some(total_money),
        series,
        top_services,
        top_packages,
    })
}

/// `GET /reports/ledger?from=&to=&cursor=&limit=` (`report.revenue.read`): the revenue book,
/// in book order (day, time), with every day's subtotal.
pub async fn ledger(
    pool: &PgPool,
    actor: &Actor,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    after: Option<&str>,
    limit: Option<u32>,
) -> Result<LedgerPage, AppError> {
    actor.require(Permission::ReportRevenueRead)?;
    let limit = cursor::limit(limit)?;
    let after = match after {
        Some(raw) => {
            let p = cursor::decode(raw, 2)?;
            Some((cursor::date(&p[0])?, cursor::uuid(&p[1])?))
        }
        None => None,
    };
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let today = today(&mut tx).await?;
    let from = from.unwrap_or(today);
    let (from, to) = window(
        Some(from),
        Some(to.unwrap_or(from)),
        today,
        1,
        MAX_LEDGER_DAYS,
    )?;
    let tenant = tx.tenant().as_uuid();
    let (a_day, a_seq) = after.map_or((None, None), |(d, q)| (Some(d), Some(q)));

    let mut rows = sqlx::query!(
        r#"SELECT b.id AS "id!", b.seq AS "seq!", b.entry AS "entry!",
                  b.business_date AS "business_date!", b.at AS "at!",
                  b.customer_id AS "customer_id!", c.full_name, b.sale_kind AS "sale_kind!",
                  b.one_off, b.package, b.bank_transfer AS "bank_transfer!", b.cash AS "cash!",
                  b.unpaid AS "unpaid!", b.note, b.voided AS "voided!"
           FROM app.revenue_book b
           JOIN app.customer c ON c.tenant_id = b.tenant_id AND c.id = b.customer_id
           WHERE b.tenant_id = $1 AND b.business_date BETWEEN $2 AND $3
             AND ($4::date IS NULL OR (b.business_date, b.seq) > ($4, $5))
           ORDER BY b.business_date, b.seq
           LIMIT $6"#,
        tenant,
        from,
        to,
        a_day,
        a_seq,
        limit + 1
    )
    .fetch_all(tx.conn())
    .await?;

    let days = sqlx::query!(
        r#"SELECT b.business_date AS "date!",
                  coalesce(sum(b.bank_transfer), 0)::bigint AS "bank_transfer!",
                  coalesce(sum(b.cash), 0)::bigint AS "cash!",
                  coalesce(sum(b.unpaid), 0)::bigint AS "unpaid!",
                  count(*) AS "entries!"
           FROM app.revenue_book b
           WHERE b.tenant_id = $1 AND b.business_date BETWEEN $2 AND $3
           GROUP BY b.business_date
           ORDER BY b.business_date"#,
        tenant,
        from,
        to
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|d| LedgerDay {
        date: d.date,
        bank_transfer: d.bank_transfer,
        cash: d.cash,
        unpaid: d.unpaid,
        total: d.bank_transfer + d.cash,
        entries: d.entries,
    })
    .collect();
    tx.commit().await?;

    let more = rows.len() > usize::try_from(limit).unwrap_or(usize::MAX);
    rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
    let next_cursor = more
        .then(|| rows.last())
        .flatten()
        .map(|r| cursor::encode(&[r.business_date.to_string(), r.seq.to_string()]));
    Ok(LedgerPage {
        from,
        to,
        rows: rows
            .into_iter()
            .map(|r| LedgerRow {
                id: r.id,
                entry: match r.entry.as_str() {
                    "sale" => LedgerEntry::Sale,
                    "void" => LedgerEntry::Void,
                    _ => LedgerEntry::Refund,
                },
                business_date: r.business_date,
                at: r.at,
                customer_id: r.customer_id,
                customer_name: r.full_name,
                kind: SaleKind::parse(&r.sale_kind),
                one_off: r.one_off,
                package: r.package,
                bank_transfer: r.bank_transfer,
                cash: r.cash,
                unpaid: r.unpaid,
                note: r.note,
                voided: r.voided,
            })
            .collect(),
        days,
        next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    #[test]
    fn buckets_match_postgres_date_trunc() {
        // 2026-11-05 is a Thursday.
        assert_eq!(
            bucket_of(d("2026-11-05"), Granularity::Week),
            d("2026-11-02")
        );
        assert_eq!(
            bucket_of(d("2026-11-02"), Granularity::Week),
            d("2026-11-02")
        );
        assert_eq!(
            bucket_of(d("2026-11-08"), Granularity::Week),
            d("2026-11-02")
        );
        assert_eq!(
            bucket_of(d("2026-11-05"), Granularity::Month),
            d("2026-11-01")
        );
        assert_eq!(
            bucket_of(d("2026-11-05"), Granularity::Day),
            d("2026-11-05")
        );
    }

    #[test]
    fn windows_default_and_are_bounded() {
        let today = d("2026-11-05");
        assert_eq!(
            window(None, Some(today), today, 7, MAX_RANGE_DAYS).ok(),
            Some((d("2026-10-30"), today))
        );
        assert!(window(Some(d("2026-11-06")), Some(today), today, 7, 366).is_err());
        assert!(
            window(Some(d("2025-11-05")), Some(today), today, 7, 366).is_ok(),
            "366 days"
        );
        assert!(
            window(Some(d("2025-11-04")), Some(today), today, 7, 366).is_err(),
            "367 days"
        );
        assert_eq!(
            window(Some(today), Some(today), today, 1, MAX_LEDGER_DAYS).ok(),
            Some((today, today))
        );
    }
}
