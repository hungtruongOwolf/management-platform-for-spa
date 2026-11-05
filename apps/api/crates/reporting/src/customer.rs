//! One customer, looked back on: the timeline (visits, purchases, voids, cancellations with
//! refunds), the summary stats and the per-package usage page (the paper "sổ theo dõi liệu trình").
//! Visit notes (health data) are never read here.

use crate::cursor;
use crate::dto::{
    CustomerStats, HistoryCancellation, HistoryEvent, HistoryKind, HistoryPage, HistoryPurchase,
    HistoryPurchaseLine, HistoryQuery, HistoryRefund, HistoryVisit, HistoryVisitLine, HistoryVoid,
    LineSource, MethodSplit, PackageCounts, PackageUsage, PackageUse,
};
use crm::text::{like_escape, search_key};
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use packages::http::CustomerPackageDto;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Longest search text (existing rule for every `q`).
const MAX_QUERY_CHARS: usize = 100;

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

/// The timeline shows money and sessions, so it needs the profile, the visits and the sales.
fn require_history(actor: &Actor) -> Result<(), AppError> {
    actor.require(Permission::CustomerRead)?;
    actor.require(Permission::VisitRead)?;
    actor.require(Permission::SaleRead)
}

async fn require_customer(tx: &mut TenantTx, id: Uuid) -> Result<(), AppError> {
    if crm::service::exists_in(tx, id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

#[derive(Default, Clone, Copy)]
struct Paid {
    paid: MethodSplit,
    reversed: MethodSplit,
}

/// Payments and reversals of `sale_ids`, per sale.
async fn payments(tx: &mut TenantTx, sale_ids: &[Uuid]) -> Result<HashMap<Uuid, Paid>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let mut out: HashMap<Uuid, Paid> = HashMap::new();
    for r in sqlx::query!(
        r#"SELECT p.sale_id, p.kind, p.method, sum(p.amount)::bigint AS "amount!"
           FROM app.payment p
           WHERE p.tenant_id = $1 AND p.sale_id = ANY($2)
           GROUP BY 1, 2, 3"#,
        tenant,
        sale_ids
    )
    .fetch_all(tx.conn())
    .await?
    {
        let entry = out.entry(r.sale_id).or_default();
        let split = if r.kind == "reversal" {
            &mut entry.reversed
        } else {
            &mut entry.paid
        };
        split.add(&r.method, r.amount);
    }
    Ok(out)
}

struct Visits {
    visits: HashMap<Uuid, HistoryVisit>,
    sale_of: HashMap<Uuid, Uuid>,
    sessions: HashMap<Uuid, i64>,
}

async fn visits(tx: &mut TenantTx, ids: &[Uuid]) -> Result<Visits, AppError> {
    let tenant = tx.tenant().as_uuid();
    let mut lines: HashMap<Uuid, Vec<HistoryVisitLine>> = HashMap::new();
    let mut sessions: HashMap<Uuid, i64> = HashMap::new();
    for l in sqlx::query!(
        r#"SELECT l.visit_id, l.service_name_snapshot, sc.name AS category_name, l.source,
                  l.customer_package_id, cp.name_snapshot AS "package_name?", l.qty,
                  l.sessions_left_after, l.staff_id, st.full_name AS staff_name, l.line_value
           FROM app.visit_line l
           JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id
           JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           LEFT JOIN app.customer_package cp
             ON cp.tenant_id = l.tenant_id AND cp.id = l.customer_package_id
           WHERE l.tenant_id = $1 AND l.visit_id = ANY($2)
           ORDER BY l.visit_id, l.position"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?
    {
        let source = if l.source == "package" {
            *sessions.entry(l.visit_id).or_default() += i64::from(l.qty);
            LineSource::Package
        } else {
            LineSource::OneOff
        };
        lines.entry(l.visit_id).or_default().push(HistoryVisitLine {
            service_name: l.service_name_snapshot,
            category_name: l.category_name,
            source,
            customer_package_id: l.customer_package_id,
            package_name: l.package_name,
            qty: l.qty,
            sessions_left_after: l.sessions_left_after,
            staff_id: l.staff_id,
            staff_name: l.staff_name,
            amount: if source == LineSource::OneOff {
                l.line_value
            } else {
                0
            },
        });
    }
    let heads = sqlx::query!(
        r#"SELECT v.id, v.status, v.check_in_at, v.void_reason, v.voided_at, v.sale_id
           FROM app.visit v WHERE v.tenant_id = $1 AND v.id = ANY($2)"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?;
    let sale_of: HashMap<Uuid, Uuid> = heads
        .iter()
        .filter_map(|h| h.sale_id.map(|s| (h.id, s)))
        .collect();
    let sale_ids: Vec<Uuid> = sale_of.values().copied().collect();
    let paid = payments(tx, &sale_ids).await?;
    let visits = heads
        .into_iter()
        .map(|h| {
            let money = h
                .sale_id
                .and_then(|s| paid.get(&s).copied())
                .unwrap_or_default();
            (
                h.id,
                HistoryVisit {
                    id: h.id,
                    voided: h.status == "voided",
                    check_in_at: h.check_in_at,
                    lines: lines.remove(&h.id).unwrap_or_default(),
                    paid: money.paid,
                    void_reason: h.void_reason,
                    voided_at: h.voided_at,
                },
            )
        })
        .collect();
    Ok(Visits {
        visits,
        sale_of,
        sessions,
    })
}

async fn purchases(
    tx: &mut TenantTx,
    ids: &[Uuid],
) -> Result<(HashMap<Uuid, HistoryPurchase>, HashMap<Uuid, Paid>), AppError> {
    let tenant = tx.tenant().as_uuid();
    let mut lines: HashMap<Uuid, Vec<HistoryPurchaseLine>> = HashMap::new();
    for l in sqlx::query!(
        r#"SELECT sl.sale_id, sl.description_snapshot, sl.qty, sl.line_total, sl.customer_package_id
           FROM app.sale_line sl
           WHERE sl.tenant_id = $1 AND sl.sale_id = ANY($2)
           ORDER BY sl.sale_id, sl.position"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?
    {
        lines
            .entry(l.sale_id)
            .or_default()
            .push(HistoryPurchaseLine {
                description: l.description_snapshot,
                qty: l.qty,
                line_total: l.line_total,
                customer_package_id: l.customer_package_id,
            });
    }
    let paid = payments(tx, ids).await?;
    let sales = sqlx::query!(
        r#"SELECT s.id, s.status, s.subtotal, s.discount_amount, s.discount_reason, s.total,
                  s.void_reason, s.voided_at, st.full_name AS "closer_name?"
           FROM app.sale s
           LEFT JOIN app.staff st ON st.tenant_id = s.tenant_id AND st.id = s.closer_staff_id
           WHERE s.tenant_id = $1 AND s.id = ANY($2)"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|s| {
        (
            s.id,
            HistoryPurchase {
                sale_id: s.id,
                voided: s.status == "voided",
                lines: lines.remove(&s.id).unwrap_or_default(),
                subtotal: s.subtotal,
                discount_amount: s.discount_amount,
                discount_reason: s.discount_reason,
                total: s.total,
                paid: paid.get(&s.id).map(|p| p.paid).unwrap_or_default(),
                closer_name: s.closer_name,
                void_reason: s.void_reason,
                voided_at: s.voided_at,
            },
        )
    })
    .collect();
    Ok((sales, paid))
}

async fn cancellations(
    tx: &mut TenantTx,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, HistoryCancellation>, AppError> {
    let tenant = tx.tenant().as_uuid();
    Ok(sqlx::query!(
        r#"SELECT cp.id, cp.name_snapshot, cp.cancel_reason,
                  r.amount AS "amount?", r.method AS "method?", r.business_date AS "refund_date?"
           FROM app.customer_package cp
           LEFT JOIN app.refund r ON r.tenant_id = cp.tenant_id AND r.customer_package_id = cp.id
           WHERE cp.tenant_id = $1 AND cp.id = ANY($2)"#,
        tenant,
        ids
    )
    .fetch_all(tx.conn())
    .await?
    .into_iter()
    .map(|c| {
        let refund = match (c.amount, c.method, c.refund_date) {
            (Some(amount), Some(method), Some(business_date)) => Some(HistoryRefund {
                amount,
                method,
                business_date,
            }),
            _ => None,
        };
        (
            c.id,
            HistoryCancellation {
                customer_package_id: c.id,
                package_name: c.name_snapshot,
                reason: c.cancel_reason.unwrap_or_default(),
                refund,
            },
        )
    })
    .collect())
}

fn kind(s: &str) -> Result<HistoryKind, AppError> {
    Ok(match s {
        "visit" => HistoryKind::Visit,
        "visit_voided" => HistoryKind::VisitVoided,
        "purchase" => HistoryKind::Purchase,
        "purchase_voided" => HistoryKind::PurchaseVoided,
        "package_cancelled" => HistoryKind::PackageCancelled,
        other => return Err(AppError::Internal(format!("history kind {other}"))),
    })
}

/// `q` of the history search → a `LIKE` pattern over accent-folded names (`app.search_fold`).
fn text_filter(q: Option<&str>) -> Result<Option<String>, AppError> {
    let q = q.map(str::trim).unwrap_or_default();
    if q.is_empty() {
        return Ok(None);
    }
    if q.chars().count() > MAX_QUERY_CHARS {
        return Err(field("q", "too_long"));
    }
    let key = search_key(q);
    if key.chars().count() < 2 {
        return Err(field("q", "too_short"));
    }
    Ok(Some(format!("%{}%", like_escape(&key))))
}

fn kind_sql(kind: HistoryKind) -> &'static str {
    match kind {
        HistoryKind::Visit => "visit",
        HistoryKind::VisitVoided => "visit_voided",
        HistoryKind::Purchase => "purchase",
        HistoryKind::PurchaseVoided => "purchase_voided",
        HistoryKind::PackageCancelled => "package_cancelled",
    }
}

/// `GET /customers/{id}/history?q=&kind=&from=&to=&cursor=&limit=` (`customer.read` +
/// `visit.read` + `sale.read`): newest business day first, then newest first within the day (a
/// back-dated record keeps its day even though its `created_at` is when it was typed). Visits
/// carry their one-off payment (the visit's own sale is not a separate purchase); a void is its
/// own event at the moment it happened.
///
/// Filters: `kind`; `from` / `to` on the event's business day; `q` (accent-insensitive) matches a
/// visit by its services, their categories, packages and technicians, a purchase by its lines
/// and closer, a cancellation by its package name.
pub async fn history(
    pool: &PgPool,
    actor: &Actor,
    customer_id: Uuid,
    query: &HistoryQuery,
) -> Result<HistoryPage, AppError> {
    require_history(actor)?;
    let limit = cursor::limit(query.limit)?;
    let pattern = text_filter(query.q.as_deref())?;
    if let (Some(from), Some(to)) = (query.from, query.to)
        && from > to
    {
        return Err(field("from", "after_to"));
    }
    let only_kind = query.kind.map(kind_sql);
    let after = match query.cursor.as_deref() {
        Some(raw) => {
            let p = cursor::decode(raw, 4)?;
            Some((
                cursor::date(&p[0])?,
                cursor::instant(&p[1])?,
                cursor::integer(&p[2])?,
                cursor::uuid(&p[3])?,
            ))
        }
        None => None,
    };
    let (a_day, a_at, a_rank, a_id) = after.map_or((None, None, None, None), |(d, t, r, i)| {
        (Some(d), Some(t), Some(r), Some(i))
    });
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    require_customer(&mut tx, customer_id).await?;
    let tz = kernel::tenant::profile(&mut tx).await?.timezone.name();
    let tenant = tx.tenant().as_uuid();
    let mut heads = sqlx::query!(
        r#"WITH ev AS (
             SELECT 'visit'::text AS kind, 0 AS rank, v.check_in_at AS at, v.business_date, v.id
             FROM app.visit v WHERE v.tenant_id = $1 AND v.customer_id = $2
             UNION ALL
             SELECT 'visit_voided', 1, v.voided_at, (v.voided_at AT TIME ZONE $3)::date, v.id
             FROM app.visit v
             WHERE v.tenant_id = $1 AND v.customer_id = $2 AND v.status = 'voided'
             UNION ALL
             SELECT 'purchase', 2, s.created_at, s.business_date, s.id
             FROM app.sale s
             WHERE s.tenant_id = $1 AND s.customer_id = $2
               AND NOT EXISTS (SELECT 1 FROM app.visit v WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id)
             UNION ALL
             SELECT 'purchase_voided', 3, s.voided_at, (s.voided_at AT TIME ZONE $3)::date, s.id
             FROM app.sale s
             WHERE s.tenant_id = $1 AND s.customer_id = $2 AND s.status = 'voided'
               AND NOT EXISTS (SELECT 1 FROM app.visit v WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id)
             UNION ALL
             SELECT 'package_cancelled', 4, cp.cancelled_at, (cp.cancelled_at AT TIME ZONE $3)::date, cp.id
             FROM app.customer_package cp
             LEFT JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id
             WHERE cp.tenant_id = $1 AND cp.customer_id = $2 AND cp.cancelled_at IS NOT NULL
               AND (s.id IS NULL OR s.status <> 'voided')
           )
           SELECT kind AS "kind!", rank AS "rank!", at AS "at!", business_date AS "business_date!",
                  id AS "id!"
           FROM ev
           WHERE ($4::date IS NULL OR (business_date, at, rank, id) < ($4, $5, $6, $7))
             AND ($9::text IS NULL OR kind = $9)
             AND ($10::date IS NULL OR business_date >= $10)
             AND ($11::date IS NULL OR business_date <= $11)
             AND ($12::text IS NULL OR CASE
                   WHEN kind IN ('visit', 'visit_voided') THEN EXISTS (
                     SELECT 1 FROM app.visit_line l
                     JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id
                     JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
                     JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
                     LEFT JOIN app.customer_package cp
                       ON cp.tenant_id = l.tenant_id AND cp.id = l.customer_package_id
                     WHERE l.tenant_id = $1 AND l.visit_id = ev.id
                       AND (app.search_fold(l.service_name_snapshot) LIKE $12
                            OR app.search_fold(sc.name) LIKE $12
                            OR app.search_fold(st.full_name) LIKE $12
                            OR app.search_fold(cp.name_snapshot) LIKE $12))
                   WHEN kind IN ('purchase', 'purchase_voided') THEN EXISTS (
                     SELECT 1 FROM app.sale_line sl
                     WHERE sl.tenant_id = $1 AND sl.sale_id = ev.id
                       AND app.search_fold(sl.description_snapshot) LIKE $12)
                     OR EXISTS (
                     SELECT 1 FROM app.sale s
                     JOIN app.staff st ON st.tenant_id = s.tenant_id AND st.id = s.closer_staff_id
                     WHERE s.tenant_id = $1 AND s.id = ev.id AND app.search_fold(st.full_name) LIKE $12)
                   ELSE EXISTS (
                     SELECT 1 FROM app.customer_package cp
                     WHERE cp.tenant_id = $1 AND cp.id = ev.id
                       AND app.search_fold(cp.name_snapshot) LIKE $12)
                 END)
           ORDER BY business_date DESC, at DESC, rank DESC, id DESC
           LIMIT $8"#,
        tenant,
        customer_id,
        tz,
        a_day,
        a_at,
        a_rank,
        a_id,
        limit + 1,
        only_kind,
        query.from,
        query.to,
        pattern
    )
    .fetch_all(tx.conn())
    .await?;
    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    let more = heads.len() > page;
    heads.truncate(page);

    let ids_of = |wanted: &[&str]| -> Vec<Uuid> {
        heads
            .iter()
            .filter(|h| wanted.contains(&h.kind.as_str()))
            .map(|h| h.id)
            .collect()
    };
    let visit_ids = ids_of(&["visit", "visit_voided"]);
    let sale_ids = ids_of(&["purchase", "purchase_voided"]);
    let cancel_ids = ids_of(&["package_cancelled"]);
    let mut v = visits(&mut tx, &visit_ids).await?;
    let (sales, sale_money) = purchases(&mut tx, &sale_ids).await?;
    let mut cancels = cancellations(&mut tx, &cancel_ids).await?;
    let visit_sale_ids: Vec<Uuid> = v.sale_of.values().copied().collect();
    let visit_money = payments(&mut tx, &visit_sale_ids).await?;
    tx.commit().await?;

    let next_cursor = more.then(|| heads.last()).flatten().map(|h| {
        cursor::encode(&[
            h.business_date.to_string(),
            cursor::instant_part(h.at),
            h.rank.to_string(),
            h.id.to_string(),
        ])
    });
    let mut events = Vec::with_capacity(heads.len());
    for h in heads {
        let kind = kind(&h.kind)?;
        let mut event = HistoryEvent {
            kind,
            at: h.at,
            business_date: h.business_date,
            visit: None,
            purchase: None,
            void: None,
            cancellation: None,
        };
        match kind {
            HistoryKind::Visit => event.visit = v.visits.get(&h.id).cloned(),
            HistoryKind::VisitVoided => {
                let reason = v
                    .visits
                    .get(&h.id)
                    .and_then(|x| x.void_reason.clone())
                    .unwrap_or_default();
                let reversed = v
                    .sale_of
                    .get(&h.id)
                    .and_then(|s| visit_money.get(s))
                    .map(|p| p.reversed)
                    .unwrap_or_default();
                event.void = Some(HistoryVoid {
                    id: h.id,
                    reason,
                    reversed,
                    sessions_restored: v.sessions.remove(&h.id).unwrap_or_default(),
                });
            }
            HistoryKind::Purchase => event.purchase = sales.get(&h.id).cloned(),
            HistoryKind::PurchaseVoided => {
                event.void = Some(HistoryVoid {
                    id: h.id,
                    reason: sales
                        .get(&h.id)
                        .and_then(|s| s.void_reason.clone())
                        .unwrap_or_default(),
                    reversed: sale_money
                        .get(&h.id)
                        .map(|p| p.reversed)
                        .unwrap_or_default(),
                    sessions_restored: 0,
                });
            }
            HistoryKind::PackageCancelled => event.cancellation = cancels.remove(&h.id),
        }
        events.push(event);
    }
    Ok(HistoryPage {
        customer_id,
        events,
        next_cursor,
    })
}

/// `GET /customers/{id}/summary` (`customer.read` + `visit.read` + `sale.read`).
pub async fn summary(
    pool: &PgPool,
    actor: &Actor,
    customer_id: Uuid,
) -> Result<CustomerStats, AppError> {
    require_history(actor)?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    require_customer(&mut tx, customer_id).await?;
    let today = kernel::tenant::profile(&mut tx)
        .await?
        .business_date
        .as_naive();
    let tenant = tx.tenant().as_uuid();
    let visits = sqlx::query!(
        r#"SELECT min(v.business_date) FILTER (WHERE v.status = 'completed') AS first_on,
                  max(v.business_date) FILTER (WHERE v.status = 'completed') AS last_on,
                  count(*) FILTER (WHERE v.status = 'completed') AS "visits!",
                  count(*) FILTER (WHERE v.status = 'voided') AS "voided!",
                  coalesce((SELECT sum(l.qty) FROM app.visit_line l
                            JOIN app.visit v2 ON v2.tenant_id = l.tenant_id AND v2.id = l.visit_id
                            WHERE l.tenant_id = $1 AND v2.customer_id = $2
                              AND v2.status = 'completed' AND l.source = 'package'), 0)::bigint
                    AS "sessions!"
           FROM app.visit v WHERE v.tenant_id = $1 AND v.customer_id = $2"#,
        tenant,
        customer_id
    )
    .fetch_one(tx.conn())
    .await?;
    let money = sqlx::query!(
        r#"SELECT coalesce(sum(p.amount) FILTER (WHERE p.kind = 'payment'), 0)::bigint AS "paid!",
                  coalesce(sum(p.amount) FILTER (WHERE p.kind = 'reversal'), 0)::bigint AS "voided!",
                  coalesce((SELECT sum(r.amount) FROM app.refund r
                            WHERE r.tenant_id = $1 AND r.customer_id = $2), 0)::bigint AS "refunded!"
           FROM app.payment p
           JOIN app.sale s ON s.tenant_id = p.tenant_id AND s.id = p.sale_id
           WHERE p.tenant_id = $1 AND s.customer_id = $2"#,
        tenant,
        customer_id
    )
    .fetch_one(tx.conn())
    .await?;
    let mut packages = PackageCounts::default();
    let mut sessions_remaining = 0;
    for p in sqlx::query!(
        r#"SELECT app.package_status(ps.cancelled_at, ps.sessions_remaining, ps.expires_on, $3)
                    AS "status!",
                  count(*) AS "n!", coalesce(sum(ps.sessions_remaining), 0)::bigint AS "remaining!"
           FROM app.customer_package_state ps
           WHERE ps.tenant_id = $1 AND ps.customer_id = $2
           GROUP BY 1"#,
        tenant,
        customer_id,
        today
    )
    .fetch_all(tx.conn())
    .await?
    {
        match p.status.as_str() {
            "active" => {
                packages.active = p.n;
                sessions_remaining = p.remaining;
            }
            "expired" => packages.expired = p.n,
            "exhausted" => packages.exhausted = p.n,
            _ => packages.cancelled = p.n,
        }
    }
    tx.commit().await?;
    Ok(CustomerStats {
        customer_id,
        first_visit_on: visits.first_on,
        last_visit_on: visits.last_on,
        days_since_last_visit: visits.last_on.map(|d| (today - d).num_days()),
        visit_count: visits.visits,
        voided_visit_count: visits.voided,
        sessions_used: visits.sessions,
        paid: money.paid,
        voided: money.voided,
        refunded: money.refunded,
        net_spent: money.paid - money.voided - money.refunded,
        packages,
        sessions_remaining,
    })
}

/// `GET /customer-packages/{id}/usage` (`customer.read` + `visit.read`): the package (same DTO as
/// `GET /customer-packages/{id}`) and every use of it, oldest first, voided ones flagged.
pub async fn package_usage(
    pool: &PgPool,
    actor: &Actor,
    package_id: Uuid,
) -> Result<PackageUsage, AppError> {
    actor.require(Permission::CustomerRead)?;
    actor.require(Permission::VisitRead)?;
    let package = packages::service::get(pool, actor, package_id).await?;
    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let tenant = tx.tenant().as_uuid();
    let uses = sqlx::query_as!(
        PackageUse,
        r#"SELECT v.id AS visit_id, l.id AS visit_line_id, v.business_date, v.check_in_at,
                  l.customer_package_item_id AS "item_id!", l.service_name_snapshot AS service_name,
                  sc.name AS category_name, l.qty, l.sessions_left_after, l.staff_id,
                  st.full_name AS staff_name, v.status = 'voided' AS "voided!"
           FROM app.visit_line l
           JOIN app.visit v ON v.tenant_id = l.tenant_id AND v.id = l.visit_id
           JOIN app.staff st ON st.tenant_id = l.tenant_id AND st.id = l.staff_id
           JOIN app.service sv ON sv.tenant_id = l.tenant_id AND sv.id = l.service_id
           JOIN app.service_category sc ON sc.tenant_id = sv.tenant_id AND sc.id = sv.category_id
           WHERE l.tenant_id = $1 AND l.customer_package_id = $2
           ORDER BY v.check_in_at, l.position"#,
        tenant,
        package_id
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(PackageUsage {
        package: CustomerPackageDto::from(package),
        uses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_text_is_folded_escaped_and_bounded() -> Result<(), AppError> {
        assert_eq!(text_filter(None)?, None);
        assert_eq!(text_filter(Some("  "))?, None);
        assert_eq!(text_filter(Some("Gội ĐẦU"))?, Some("%goi dau%".to_owned()));
        assert_eq!(text_filter(Some("50%"))?, Some("%50\\%%".to_owned()));
        assert!(text_filter(Some("a")).is_err());
        assert!(text_filter(Some(&"x".repeat(101))).is_err());
        Ok(())
    }
}
