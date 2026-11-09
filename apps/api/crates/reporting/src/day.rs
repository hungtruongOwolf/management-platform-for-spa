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
use crm::http::gender_dto;
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Longest `GET /reports/range` window by day or week: a full (leap) year.
pub const MAX_RANGE_DAYS: u64 = 366;
/// Longest window by month or year: two (leap) years, within the 300 ms budget (the bench reads
/// one busy year in ~120 ms, two in ~230 ms).
pub const MAX_LONG_RANGE_DAYS: u64 = 2 * 366;
/// With `report.long_range` (owner, EM Q5): five (leap) years, computed on demand. Rarely used,
/// so it has its own, looser budget (p95 ≤ 1.5 s on the 3-year bench) instead of a rollup table.
pub const MAX_OWNER_RANGE_DAYS: u64 = 5 * 366;
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

/// Longest span for a granularity; `long_range` = the caller holds `report.long_range`.
const fn max_days(g: Granularity, long_range: bool) -> u64 {
    match (g, long_range) {
        (Granularity::Day | Granularity::Week, _) => MAX_RANGE_DAYS,
        (Granularity::Month | Granularity::Year, false) => MAX_LONG_RANGE_DAYS,
        (Granularity::Month | Granularity::Year, true) => MAX_OWNER_RANGE_DAYS,
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
        RangePreset::Last2Years => (jan_first(today.year() - 1)?, today, Granularity::Month),
        RangePreset::Last3Years => (jan_first(today.year() - 2)?, today, Granularity::Month),
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
    /// Completed visit lines of the whole span, per service and per technician.
    services: Vec<LineTotals>,
    staff: Vec<LineTotals>,
    /// Distinct customers with a completed visit over the whole span.
    customers: i64,
}

struct LineTotals {
    id: Uuid,
    /// Distinct visits (technicians only).
    visits: i64,
    sessions: i64,
    one_off: i64,
    one_off_revenue: i64,
}

impl Books {
    /// Reads the books of `from..=to`, grouped by `granularity`. Money only when `with_money`.
    /// Always a range scan on the day indexes (the daily dashboard reads its 8-day span), so a
    /// five-year report never ships 1 800 dates and no plan depends on an optional array.
    async fn read(
        tx: &mut TenantTx,
        (from, to): (NaiveDate, NaiveDate),
        granularity: Granularity,
        with_money: bool,
    ) -> Result<Self, AppError> {
        let tenant = tx.tenant().as_uuid();
        let unit = granularity.as_sql();
        let mut books = Self::default();

        // Every visit-side figure from one scan of the range's visit lines (materialised once,
        // so the base tables and their RLS checks are read once): per bucket, per service, per
        // technician and the distinct customers of the whole range. Every visit has ≥ 1 line.
        for r in sqlx::query!(
            r#"WITH vl AS MATERIALIZED (
                 SELECT date_trunc($3, v.business_date::timestamp)::date AS bucket, v.id AS visit_id,
                        v.customer_id, v.status = 'completed' AS done, l.service_id, l.staff_id,
                        l.source, l.qty, l.line_value,
                        -- Replaced by "Sửa lượt": shown as "đã sửa", not as a cancellation.
                        EXISTS (SELECT 1 FROM app.visit n
                                WHERE n.tenant_id = v.tenant_id AND n.corrects_visit_id = v.id) AS corrected
                 FROM app.visit v
                 JOIN app.visit_line l ON l.tenant_id = v.tenant_id AND l.visit_id = v.id
                 WHERE v.tenant_id = $1 AND v.business_date BETWEEN $2 AND $4
               )
               SELECT 'bucket' AS tag, bucket, NULL::uuid AS key,
                      count(DISTINCT visit_id) FILTER (WHERE done) AS visits,
                      count(DISTINCT customer_id) FILTER (WHERE done) AS customers,
                      count(DISTINCT visit_id) FILTER (WHERE NOT done AND NOT corrected) AS voided,
                      count(DISTINCT visit_id) FILTER (WHERE NOT done AND corrected) AS corrected,
                      coalesce(sum(qty) FILTER (WHERE done AND source = 'package'), 0)::bigint AS sessions,
                      coalesce(sum(qty) FILTER (WHERE done AND source = 'one_off'), 0)::bigint AS one_off,
                      coalesce(sum(line_value) FILTER (WHERE done AND source = 'one_off'), 0)::bigint AS revenue
               FROM vl GROUP BY bucket
               UNION ALL
               SELECT 'service', NULL, service_id, 0, 0, 0, 0,
                      coalesce(sum(qty) FILTER (WHERE source = 'package'), 0)::bigint,
                      coalesce(sum(qty) FILTER (WHERE source = 'one_off'), 0)::bigint,
                      coalesce(sum(line_value) FILTER (WHERE source = 'one_off'), 0)::bigint
               FROM vl WHERE done GROUP BY service_id
               UNION ALL
               SELECT 'staff', NULL, staff_id, count(DISTINCT visit_id), 0, 0, 0,
                      coalesce(sum(qty) FILTER (WHERE source = 'package'), 0)::bigint,
                      coalesce(sum(qty) FILTER (WHERE source = 'one_off'), 0)::bigint, 0
               FROM vl WHERE done GROUP BY staff_id
               UNION ALL
               SELECT 'all', NULL, NULL, 0, count(DISTINCT customer_id), 0, 0, 0, 0, 0
               FROM vl WHERE done"#,
            tenant,
            from,
            unit,
            to
        )
        .fetch_all(tx.conn())
        .await?
        {
            let n = |v: Option<i64>| v.unwrap_or_default();
            match (r.tag.as_deref(), r.bucket, r.key) {
                (Some("bucket"), Some(bucket), _) => {
                    let c = books.counts.entry(bucket).or_default();
                    c.visits = n(r.visits);
                    c.customers = n(r.customers);
                    c.voided_visits = n(r.voided);
                    c.corrected_visits = n(r.corrected);
                    c.sessions_used = n(r.sessions);
                    c.one_off_services = n(r.one_off);
                }
                (Some("service"), _, Some(id)) => books.services.push(LineTotals {
                    id,
                    visits: 0,
                    sessions: n(r.sessions),
                    one_off: n(r.one_off),
                    one_off_revenue: n(r.revenue),
                }),
                (Some("staff"), _, Some(id)) => books.staff.push(LineTotals {
                    id,
                    visits: n(r.visits),
                    sessions: n(r.sessions),
                    one_off: n(r.one_off),
                    one_off_revenue: 0,
                }),
                (Some("all"), _, _) => books.customers = n(r.customers),
                _ => {}
            }
        }

        for r in sqlx::query!(
            // ADR-0019 D7: a package refunded in full since does not count as sold.
            r#"SELECT date_trunc($3, s.business_date::timestamp)::date AS "bucket!",
                      count(*) FILTER (WHERE NOT (m.refunded > 0 AND m.refunded >= m.paid)) AS "sold!"
               FROM app.sale s
               JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
               CROSS JOIN LATERAL (
                 SELECT CASE WHEN s.source = 'paper_book' THEN s.amount_paid ELSE cp.sold_price END AS paid,
                        coalesce((SELECT sum(r.amount) FROM app.refund r
                                  WHERE r.tenant_id = cp.tenant_id AND r.customer_package_id = cp.id), 0)::bigint AS refunded
               ) m
               WHERE s.tenant_id = $1 AND s.business_date BETWEEN $2 AND $4 AND s.status = 'open'
               GROUP BY 1"#,
            tenant,
            from,
            unit,
            to
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
                   WHERE p.tenant_id = $1 AND p.business_date BETWEEN $2 AND $4
                   GROUP BY 1, 2, 3, 4
                   UNION ALL
                   SELECT date_trunc($3, r.business_date::timestamp)::date, 'refund', r.method,
                          'package', sum(r.amount)::bigint
                   FROM app.refund r
                   WHERE r.tenant_id = $1 AND r.business_date BETWEEN $2 AND $4
                   GROUP BY 1, 2, 3, 4"#,
                tenant,
                from,
                unit,
                to
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let date = match date {
        Some(d) => d,
        None => today(&mut tx).await?,
    };
    let (yesterday, last_week) = (minus_days(date, 1)?, minus_days(date, 7)?);
    let books = Books::read(&mut tx, (last_week, date), Granularity::Day, with_money).await?;
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
    long_range: bool,
) -> Result<(NaiveDate, NaiveDate, Granularity), AppError> {
    let (from, to, granularity) = match query.preset {
        Some(_) if query.from.is_some() || query.to.is_some() => {
            return Err(field("preset", "conflicts_with_range"));
        }
        // The 3- and 5-year presets are the owner's long view, whatever the calendar day.
        Some(RangePreset::Last3Years | RangePreset::Last5Years) if !long_range => {
            return Err(field("to", "range_too_long"));
        }
        Some(preset) => {
            let (from, to, default) = preset_window(preset, today)?;
            (Some(from), Some(to), query.granularity.unwrap_or(default))
        }
        None => (query.from, query.to, query.granularity.unwrap_or_default()),
    };
    let (from, to) = window(from, to, today, 7, max_days(granularity, long_range))?;
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
    let today = today(&mut tx).await?;
    let (from, to, granularity) =
        range_window(query, today, actor.can(Permission::ReportLongRange))?;
    let all_days = days(from, to);
    let books = Books::read(&mut tx, (from, to), granularity, with_money).await?;
    let tenant = tx.tenant().as_uuid();

    // Names for the per-service / per-technician totals the books already summed.
    let service_ids: Vec<Uuid> = books.services.iter().map(|s| s.id).collect();
    let names: HashMap<Uuid, (String, String)> = sqlx::query!(
        r#"SELECT s.id, s.name, sc.name AS category_name
           FROM app.service s
           JOIN app.service_category sc ON sc.tenant_id = s.tenant_id AND sc.id = s.category_id
           WHERE s.tenant_id = $1 AND s.id = ANY($2)"#,
        tenant,
        &service_ids
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|r| (r.id, (r.name, r.category_name)))
    .collect();
    let mut top_services: Vec<TopService> = books
        .services
        .iter()
        .map(|t| {
            let (name, category_name) = names.get(&t.id).cloned().unwrap_or_default();
            TopService {
                service_id: t.id,
                name,
                category_name,
                sessions_used: t.sessions,
                one_off_services: t.one_off,
                one_off_revenue: with_money.then_some(t.one_off_revenue),
            }
        })
        .collect();
    top_services.sort_by(|a, b| {
        (b.sessions_used + b.one_off_services)
            .cmp(&(a.sessions_used + a.one_off_services))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.service_id.cmp(&b.service_id))
    });
    top_services.truncate(usize::try_from(TOP).unwrap_or(10));

    let staff_ids: Vec<Uuid> = books.staff.iter().map(|s| s.id).collect();
    let staff_names: HashMap<Uuid, String> = sqlx::query!(
        "SELECT id, full_name FROM app.staff WHERE tenant_id = $1 AND id = ANY($2)",
        tenant,
        &staff_ids
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|r| (r.id, r.full_name))
    .collect();
    let mut technicians: Vec<TechnicianDay> = books
        .staff
        .iter()
        .map(|t| TechnicianDay {
            staff_id: t.id,
            name: staff_names.get(&t.id).cloned().unwrap_or_default(),
            visits: t.visits,
            sessions_used: t.sessions,
            one_off_services: t.one_off,
        })
        .collect();
    technicians.sort_by(|a, b| {
        b.visits
            .cmp(&a.visits)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.staff_id.cmp(&b.staff_id))
    });
    let customers = books.customers;

    // ADR-0019 D7: a paper-book package without a plan is its own row (by name, not one "no plan"
    // row); revenue is what was paid for it (the paper amount, or the price after discount), net
    // of any refund since; a package refunded in full since does not count as sold (a row left
    // with nothing sold and no money is dropped).
    let top_packages = sqlx::query!(
        r#"SELECT cp.plan_id, min(cp.name_snapshot) AS "name!",
                  count(*) FILTER (WHERE NOT (m.refunded > 0 AND m.refunded >= m.paid)) AS "sold!",
                  coalesce(sum(m.paid - m.refunded), 0)::bigint AS "revenue!"
           FROM app.sale s
           JOIN app.customer_package cp ON cp.tenant_id = s.tenant_id AND cp.sale_id = s.id
           CROSS JOIN LATERAL (
             SELECT CASE WHEN s.source = 'paper_book' THEN s.amount_paid ELSE cp.sold_price END AS paid,
                    coalesce((SELECT sum(r.amount) FROM app.refund r
                              WHERE r.tenant_id = cp.tenant_id AND r.customer_package_id = cp.id), 0)::bigint AS refunded
           ) m
           WHERE s.tenant_id = $1 AND s.business_date BETWEEN $2 AND $3 AND s.status = 'open'
           GROUP BY cp.plan_id, CASE WHEN cp.plan_id IS NULL THEN cp.name_snapshot END
           HAVING count(*) FILTER (WHERE NOT (m.refunded > 0 AND m.refunded >= m.paid)) > 0
               OR sum(m.paid - m.refunded) <> 0
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
        technicians,
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
    let mut tx = TenantTx::for_actor(pool, actor).await?;
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
                  c.id AS "customer_id!", app.customer_label(c.full_name) AS "full_name!",
                  c.gender, b.sale_kind AS "sale_kind!",
                  CASE WHEN b.entry = 'refund' THEN
                    (SELECT app.customer_label(o.full_name) FROM app.customer o
                     WHERE o.tenant_id = b.tenant_id AND o.id = b.customer_id AND o.id <> c.id)
                  ELSE
                    (SELECT string_agg(DISTINCT app.customer_label(o.full_name), ', ')
                     FROM app.customer_package cp
                     JOIN app.customer o ON o.tenant_id = cp.tenant_id AND o.id = cp.customer_id
                     WHERE cp.tenant_id = b.tenant_id AND cp.sale_id = b.sale_id
                       AND cp.customer_id <> b.customer_id)
                  END AS "beneficiary_name?",
                  b.one_off, b.package, b.bank_transfer AS "bank_transfer!", b.cash AS "cash!",
                  b.unpaid AS "unpaid!", b.note, b.voided AS "voided!", b.visit_id, b.upsell_by
           FROM app.revenue_book b
           -- A refund is the payer's money back (ADR-0019 D6): its row names who paid for the
           -- package (the refund row keeps the package's owner, shown as the beneficiary).
           LEFT JOIN app.customer_package rcp
             ON b.entry = 'refund' AND rcp.tenant_id = b.tenant_id AND rcp.id = b.customer_package_id
           LEFT JOIN app.sale rs ON rs.tenant_id = rcp.tenant_id AND rs.id = rcp.sale_id
           JOIN app.customer c ON c.tenant_id = b.tenant_id AND c.id = coalesce(rs.customer_id, b.customer_id)
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
                customer_gender: gender_dto(r.gender.as_deref()),
                beneficiary_name: r.beneficiary_name,
                kind: SaleKind::parse(&r.sale_kind),

                one_off: r.one_off,
                package: r.package,
                bank_transfer: r.bank_transfer,
                cash: r.cash,
                unpaid: r.unpaid,
                note: r.note,
                voided: r.voided,
                visit_id: r.visit_id,
                upsell_by: r.upsell_by,
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
    fn presets_follow_the_calendar() -> Result<(), AppError> {
        let today = d("2026-03-31");
        assert_eq!(
            preset_window(RangePreset::LastMonth, today)?,
            (d("2026-02-01"), d("2026-02-28"), Granularity::Day)
        );
        assert_eq!(
            preset_window(RangePreset::Last12Months, today)?,
            (d("2025-04-01"), today, Granularity::Month)
        );
        assert_eq!(
            preset_window(RangePreset::Last2Years, today)?,
            (d("2025-01-01"), today, Granularity::Month)
        );
        assert_eq!(
            preset_window(RangePreset::LastMonth, d("2026-01-15"))?,
            (d("2025-12-01"), d("2025-12-31"), Granularity::Day),
            "across the new year"
        );
        assert_eq!(
            bucket_of(d("2026-11-05"), Granularity::Year),
            d("2026-01-01")
        );
        // Every preset fits its granularity's limit, even on the longest days of the calendar.
        for day in [d("2028-12-31"), d("2027-01-01"), d("2026-02-28")] {
            for p in [
                RangePreset::Last7Days,
                RangePreset::Last30Days,
                RangePreset::ThisMonth,
                RangePreset::LastMonth,
                RangePreset::ThisYear,
                RangePreset::Last12Months,
                RangePreset::Last2Years,
            ] {
                let (from, to, g) = preset_window(p, day)?;
                assert!(
                    window(Some(from), Some(to), day, 7, max_days(g, false)).is_ok(),
                    "{p:?} {day}"
                );
            }
            // The owner-only presets fit the owner's limit and are refused to everyone else.
            for p in [RangePreset::Last3Years, RangePreset::Last5Years] {
                let query = RangeQuery {
                    from: None,
                    to: None,
                    granularity: None,
                    preset: Some(p),
                };
                assert!(range_window(&query, day, true).is_ok(), "{p:?} {day}");
                assert!(range_window(&query, day, false).is_err(), "{p:?} {day}");
            }
        }
        Ok(())
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
