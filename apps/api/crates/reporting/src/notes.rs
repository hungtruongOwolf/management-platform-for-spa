//! "Nhật ký ghi chú" (spec management-views §10, EM 2026-11-07): every note written in the app,
//! newest first, for the owner and the manager to read back in one place.
//!
//! Nothing is copied: one query unions the notes where they live. Each source keeps the read rule
//! it already has on its own screen ([`Allowed`]), passed to SQL as a flag per branch, so a kind the
//! caller may not read is never selected (deny by default). Encrypted notes (session notes, health
//! notes) are decrypted here, with the same associated data their writers use, only for callers
//! entitled to them, and never logged.
//!
//! **Search over encrypted notes** (the decision, spec §10.4): the database cannot search a
//! ciphertext, and decrypting every note of the spa per request does not scale. Plain notes are
//! filtered in SQL; encrypted notes inside the filtered date range are decrypted and matched here,
//! scanning at most [`SCAN_CAP`] notes per request. A page that hits the cap before filling returns
//! what it found with `scan_capped` and a cursor that continues the scan further back.

use crate::cursor;
use crate::notes_dto::{NoteEntry, NoteKind, NotesPage, NotesQuery};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use crm::text::{like_escape, search_key};
use kernel::crypto::FieldCipher;
use kernel::{Actor, AppError, FieldError, Permission, TenantTx};
use sqlx::PgPool;
use uuid::Uuid;

/// Longest date range of one request (a year, plus a leap day).
const MAX_RANGE_DAYS: i64 = 366;
/// Default range: the last 30 days, today included.
const DEFAULT_RANGE_DAYS: i64 = 30;
/// Notes scanned at most per request when a search has to decrypt.
pub const SCAN_CAP: usize = 500;
/// Rows fetched per round while a search scans.
const SEARCH_BATCH: i64 = 100;
const MAX_QUERY_CHARS: usize = 100;

fn field(name: &'static str, code: &'static str) -> AppError {
    AppError::Validation(vec![FieldError::new(name, code)])
}

/// Which kinds the caller may read: exactly the rule each note has where it is written or shown
/// today (deny by default). `discount` has two sources with different rules, kept apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allowed {
    granted: [bool; NoteKind::ALL.len()],
    /// A visit's sale: the day list (`GET /visits`, `visit.read`) shows its discount reason.
    discount_on_visit: bool,
    /// Any other sale: `GET /sales/{id}` (`sale.read`), with the customer's name (`customer.read`).
    discount_on_sale: bool,
}

impl Allowed {
    pub fn of(actor: &Actor) -> Self {
        let can = |p| actor.can(p);
        let health = can(Permission::CustomerHealthNotesRead);
        let timeline = can(Permission::CustomerRead)
            && can(Permission::VisitRead)
            && can(Permission::SaleRead);
        let discount_on_visit = can(Permission::VisitRead);
        let discount_on_sale = can(Permission::SaleRead) && can(Permission::CustomerRead);
        let rule = |kind: NoteKind| match kind {
            NoteKind::VisitNote => can(Permission::VisitRead) && health,
            NoteKind::CustomerNote | NoteKind::FollowUp => can(Permission::CustomerRead),
            NoteKind::HealthNote => can(Permission::CustomerRead) && health,
            NoteKind::AttendanceNote => can(Permission::AttendanceRecord),
            NoteKind::ReviewNote | NoteKind::Adjustment | NoteKind::EvaluationNote => {
                can(Permission::EvaluationRecord)
            }
            NoteKind::Void | NoteKind::PackageExtension => timeline,
            NoteKind::EmploymentEnd | NoteKind::StaffNote => can(Permission::StaffManage),
            NoteKind::AppointmentNote | NoteKind::AppointmentReason => {
                can(Permission::AppointmentManage)
            }
            NoteKind::Discount => discount_on_visit || discount_on_sale,
        };
        Allowed {
            granted: NoteKind::ALL.map(rule),
            discount_on_visit,
            discount_on_sale,
        }
    }

    pub fn may(self, kind: NoteKind) -> bool {
        self.granted[kind as usize]
    }

    /// Narrowed to one kind when the filter asks for it (never widened).
    fn only(self, kind: Option<NoteKind>) -> Self {
        let Some(k) = kind else { return self };
        let keep = |x: NoteKind| self.may(x) && x == k;
        Allowed {
            granted: NoteKind::ALL.map(keep),
            discount_on_visit: self.discount_on_visit && k == NoteKind::Discount,
            discount_on_sale: self.discount_on_sale && k == NoteKind::Discount,
        }
    }
}

/// `q` → the folded key (Rust side, for decrypted notes) and its `LIKE` pattern (SQL side).
fn text_filter(q: Option<&str>) -> Result<Option<(String, String)>, AppError> {
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
    let pattern = format!("%{}%", like_escape(&key));
    Ok(Some((key, pattern)))
}

/// `from` / `to` with their defaults and bounds.
fn range(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), AppError> {
    let to = to.unwrap_or(today);
    let from = from.unwrap_or(to - Duration::days(DEFAULT_RANGE_DAYS - 1));
    if from > to {
        return Err(field("from", "after_to"));
    }
    if (to - from).num_days() >= MAX_RANGE_DAYS {
        return Err(field("to", "range_too_long"));
    }
    Ok((from, to))
}

struct NoteRow {
    kind: String,
    at: DateTime<Utc>,
    day: NaiveDate,
    key: String,
    body: Option<String>,
    ct: Option<Vec<u8>>,
    customer_id: Option<Uuid>,
    customer_name: Option<String>,
    staff_id: Option<Uuid>,
    staff_name: Option<String>,
    author_name: Option<String>,
    visit_id: Option<Uuid>,
    detail: Option<String>,
    about: Option<NaiveDate>,
    name_hit: bool,
}

struct Filters<'a> {
    from: NaiveDate,
    to: NaiveDate,
    allowed: Allowed,
    staff_id: Option<Uuid>,
    customer_id: Option<Uuid>,
    pattern: Option<&'a str>,
}

/// The sort key of the last row scanned: business day, instant, kind, key.
type After = Option<(NaiveDate, DateTime<Utc>, String, String)>;

fn encode(after: After) -> Option<String> {
    after.map(|(d, t, k, key)| cursor::encode(&[d.to_string(), cursor::instant_part(t), k, key]))
}

/// One page of note heads after `after`: newest business day first, then newest written. Every
/// branch filters by the tenant explicitly (RLS is the second wall) and by the date range on an
/// indexed column.
async fn fetch(
    tx: &mut TenantTx,
    tz: &str,
    f: &Filters<'_>,
    after: &After,
    limit: i64,
) -> Result<Vec<NoteRow>, AppError> {
    let tenant = tx.tenant().as_uuid();
    let a = f.allowed;
    let (a_day, a_at, a_kind, a_key) = match after {
        Some((d, t, k, key)) => (Some(*d), Some(*t), Some(k.clone()), Some(key.clone())),
        None => (None, None, None, None),
    };
    let rows = sqlx::query_as!(
        NoteRow,
        r#"WITH b AS (
             SELECT ($3::date::timestamp AT TIME ZONE $2) AS lo,
                    (($4::date + 1)::timestamp AT TIME ZONE $2) AS hi
           ), n AS (
             SELECT 'visit_note'::text AS kind, v.check_in_at AS at, v.business_date AS day,
                    v.id::text AS key, NULL::text AS body, v.note_ct AS ct,
                    v.customer_id AS customer_id, v.created_by AS author_id, NULL::uuid AS staff_id,
                    v.id AS visit_id, NULL::text AS detail, NULL::text AS guest, NULL::date AS about
             FROM app.visit v
             WHERE $5 AND v.tenant_id = $1 AND v.note_ct IS NOT NULL
               AND v.business_date BETWEEN $3 AND $4
             UNION ALL
             SELECT 'customer_note', c.updated_at, (c.updated_at AT TIME ZONE $2)::date, c.id::text,
                    c.note, NULL, c.id, NULL, NULL, NULL, NULL, NULL, NULL
             FROM app.customer c, b
             WHERE $6 AND c.tenant_id = $1 AND c.note IS NOT NULL AND btrim(c.note) <> ''
               AND c.anonymized_at IS NULL AND c.updated_at >= b.lo AND c.updated_at < b.hi
             UNION ALL
             SELECT 'health_note', c.updated_at, (c.updated_at AT TIME ZONE $2)::date, c.id::text,
                    NULL, c.health_note_ct, c.id, NULL, NULL, NULL, NULL, NULL, NULL
             FROM app.customer c, b
             WHERE $7 AND c.tenant_id = $1 AND c.health_note_ct IS NOT NULL
               AND c.anonymized_at IS NULL AND c.updated_at >= b.lo AND c.updated_at < b.hi
             UNION ALL
             SELECT 'attendance_note', a.updated_at, a.work_date,
                    a.staff_id::text || ':' || a.work_date::text, a.note, NULL, NULL, a.recorded_by,
                    a.staff_id, NULL, a.status, NULL, NULL
             FROM app.attendance_day a
             WHERE $8 AND a.tenant_id = $1 AND a.note IS NOT NULL
               AND a.work_date BETWEEN $3 AND $4
             UNION ALL
             SELECT 'review_note', r.updated_at, r.work_date,
                    r.staff_id::text || ':' || r.work_date::text, r.note, NULL, NULL, r.recorded_by,
                    r.staff_id, NULL, r.compliments::text || '/' || r.complaints::text, NULL, NULL
             FROM app.staff_daily_review r
             WHERE $9 AND r.tenant_id = $1 AND r.note IS NOT NULL
               AND r.work_date BETWEEN $3 AND $4
             UNION ALL
             SELECT 'adjustment', j.created_at, j.work_date, j.id::text, j.reason, NULL, NULL,
                    j.created_by, j.staff_id, NULL, j.kind, NULL, NULL
             FROM app.staff_adjustment j
             WHERE $10 AND j.tenant_id = $1 AND j.voided_at IS NULL
               AND j.work_date BETWEEN $3 AND $4
             UNION ALL
             SELECT 'follow_up', f.created_at, f.contacted_on, f.id::text, f.note, NULL,
                    f.customer_id, f.created_by, NULL, NULL, f.outcome, NULL, NULL
             FROM app.customer_follow_up f
             WHERE $11 AND f.tenant_id = $1 AND f.note IS NOT NULL
               AND f.contacted_on BETWEEN $3 AND $4
             UNION ALL
             SELECT 'void', v.voided_at, (v.voided_at AT TIME ZONE $2)::date, v.id::text,
                    v.void_reason, NULL, v.customer_id, v.voided_by, NULL, v.id,
                    CASE WHEN EXISTS (SELECT 1 FROM app.visit nv
                                      WHERE nv.tenant_id = v.tenant_id AND nv.corrects_visit_id = v.id)
                         THEN 'visit_corrected' ELSE 'visit' END, NULL, NULL
             FROM app.visit v, b
             WHERE $12 AND v.tenant_id = $1 AND v.status = 'voided'
               AND v.voided_at >= b.lo AND v.voided_at < b.hi
             UNION ALL
             SELECT 'void', s.voided_at, (s.voided_at AT TIME ZONE $2)::date, s.id::text,
                    s.void_reason, NULL, s.customer_id, s.voided_by, NULL, NULL, 'sale', NULL, NULL
             FROM app.sale s, b
             WHERE $12 AND s.tenant_id = $1 AND s.status = 'voided'
               AND s.voided_at >= b.lo AND s.voided_at < b.hi
               -- A visit's own sale is voided with the visit: its reason is the visit's.
               AND NOT EXISTS (SELECT 1 FROM app.visit v WHERE v.tenant_id = s.tenant_id AND v.sale_id = s.id)
               AND NOT EXISTS (SELECT 1 FROM app.visit_line l WHERE l.tenant_id = s.tenant_id AND l.sale_id = s.id)
             UNION ALL
             SELECT 'void', cp.cancelled_at, (cp.cancelled_at AT TIME ZONE $2)::date, cp.id::text,
                    cp.cancel_reason, NULL, cp.customer_id, cp.cancelled_by, NULL, NULL, 'package', NULL, NULL
             FROM app.customer_package cp
             LEFT JOIN app.sale s ON s.tenant_id = cp.tenant_id AND s.id = cp.sale_id, b
             WHERE $12 AND cp.tenant_id = $1 AND cp.cancelled_at IS NOT NULL
               AND cp.cancelled_at >= b.lo AND cp.cancelled_at < b.hi
               -- Cancelled by its voided sale: the sale's void says why.
               AND (s.id IS NULL OR s.status <> 'voided')
             UNION ALL
             SELECT 'employment_end', (e.ended_on::timestamp AT TIME ZONE $2), e.ended_on,
                    e.id::text, e.end_reason, NULL, NULL, e.ended_by, e.staff_id, NULL, NULL, NULL, NULL
             FROM app.staff_employment e
             WHERE $13 AND e.tenant_id = $1 AND e.end_reason IS NOT NULL
               AND e.ended_on BETWEEN $3 AND $4
             UNION ALL
             SELECT 'appointment_note', ap.created_at, (ap.created_at AT TIME ZONE $2)::date,
                    ap.id::text, ap.note, NULL, ap.customer_id, ap.created_by, ap.staff_id, NULL,
                    ap.status, ap.guest_name, ap.business_date
             FROM app.appointment ap, b
             WHERE $22 AND ap.tenant_id = $1 AND ap.note IS NOT NULL AND btrim(ap.note) <> ''
               AND ap.created_at >= b.lo AND ap.created_at < b.hi
             UNION ALL
             -- Why a booking was cancelled / the customer did not come (the reason is required).
             SELECT 'appointment_reason', ar.closed_at, (ar.closed_at AT TIME ZONE $2)::date,
                    ar.id::text, ar.status_reason, NULL, ar.customer_id, ar.closed_by, ar.staff_id,
                    NULL, ar.status, ar.guest_name, ar.business_date
             FROM app.appointment ar, b
             WHERE $28 AND ar.tenant_id = $1 AND ar.status IN ('cancelled', 'no_show')
               AND ar.closed_at >= b.lo AND ar.closed_at < b.hi
             UNION ALL
             SELECT 'evaluation_note', ev.updated_at, (ev.updated_at AT TIME ZONE $2)::date,
                    ev.staff_id::text || ':' || ev.work_date::text || ':' || part.name, part.body,
                    NULL, NULL, ev.recorded_by, ev.staff_id, NULL, part.name, NULL, ev.work_date
             FROM app.staff_evaluation ev
             CROSS JOIN LATERAL (VALUES ('comment', ev.comment), ('coaching_plan', ev.coaching_plan))
                  AS part (name, body), b
             WHERE $23 AND ev.tenant_id = $1 AND part.body IS NOT NULL
               AND ev.updated_at >= b.lo AND ev.updated_at < b.hi
             UNION ALL
             SELECT 'staff_note', sn.updated_at, (sn.updated_at AT TIME ZONE $2)::date, sn.id::text,
                    sn.note, NULL, NULL, NULL, sn.id, NULL, NULL, NULL, NULL
             FROM app.staff sn, b
             WHERE $24 AND sn.tenant_id = $1 AND sn.note IS NOT NULL
               AND sn.updated_at >= b.lo AND sn.updated_at < b.hi
             UNION ALL
             SELECT 'package_extension', x.created_at, x.business_date, x.id::text, x.reason, NULL,
                    xp.customer_id, x.created_by, NULL, NULL, xp.name_snapshot, NULL, x.expires_on
             FROM app.customer_package_extension x
             JOIN app.customer_package xp ON xp.tenant_id = x.tenant_id AND xp.id = x.customer_package_id
             WHERE $25 AND x.tenant_id = $1 AND x.business_date BETWEEN $3 AND $4
             UNION ALL
             SELECT 'discount', ds.created_at, ds.business_date, ds.id::text, ds.discount_reason,
                    NULL, ds.customer_id, ds.created_by, NULL, ds.visit_id,
                    CASE WHEN ds.visit_id IS NULL THEN 'sale' ELSE 'visit' END, NULL, NULL
             FROM app.sale ds
             WHERE ds.tenant_id = $1 AND ds.discount_reason IS NOT NULL
               AND ds.business_date BETWEEN $3 AND $4
               -- A visit's sale is shown with the visit (visit.read); any other on its own (sale.read).
               AND ((ds.visit_id IS NOT NULL AND $26) OR (ds.visit_id IS NULL AND $27))
           )
           SELECT n.kind AS "kind!", n.at AS "at!", n.day AS "day!", n.key AS "key!",
                  n.body AS "body?", n.ct AS "ct?", n.customer_id AS "customer_id?",
                  CASE WHEN n.customer_id IS NULL THEN n.guest
                       ELSE app.customer_label(c.full_name) END AS "customer_name?",
                  n.staff_id AS "staff_id?", st.full_name AS "staff_name?", u.full_name AS "author_name?",
                  n.visit_id AS "visit_id?", n.detail AS "detail?", n.about AS "about?",
                  ($17::text IS NOT NULL
                   AND (coalesce(app.search_fold(c.full_name) LIKE $17, false)
                        OR coalesce(app.search_fold(n.guest) LIKE $17, false)
                        OR coalesce(app.search_fold(st.full_name) LIKE $17, false))) AS "name_hit!"
           FROM n
           LEFT JOIN app.customer c ON c.tenant_id = $1 AND c.id = n.customer_id
           LEFT JOIN app.staff st ON st.tenant_id = $1 AND st.id = n.staff_id
           LEFT JOIN app.user_account u ON u.id = n.author_id
           WHERE ($15::uuid IS NULL OR n.customer_id = $15)
             AND ($14::uuid IS NULL OR n.staff_id = $14
                  OR n.author_id = (SELECT s2.user_id FROM app.staff s2
                                    WHERE s2.tenant_id = $1 AND s2.id = $14)
                  OR (n.visit_id IS NOT NULL AND EXISTS (
                        SELECT 1 FROM app.visit_line l
                        WHERE l.tenant_id = $1 AND l.visit_id = n.visit_id AND l.staff_id = $14)))
             AND ($17::text IS NULL OR n.ct IS NOT NULL
                  OR coalesce(app.search_fold(n.body) LIKE $17, false)
                  OR coalesce(app.search_fold(c.full_name) LIKE $17, false)
                  OR coalesce(app.search_fold(n.guest) LIKE $17, false)
                  OR coalesce(app.search_fold(st.full_name) LIKE $17, false))
             AND ($21::date IS NULL OR (n.day, n.at, n.kind, n.key) < ($21, $18, $19, $20))
           ORDER BY n.day DESC, n.at DESC, n.kind DESC, n.key DESC
           LIMIT $16"#,
        tenant,
        tz,
        f.from,
        f.to,
        a.may(NoteKind::VisitNote),
        a.may(NoteKind::CustomerNote),
        a.may(NoteKind::HealthNote),
        a.may(NoteKind::AttendanceNote),
        a.may(NoteKind::ReviewNote),
        a.may(NoteKind::Adjustment),
        a.may(NoteKind::FollowUp),
        a.may(NoteKind::Void),
        a.may(NoteKind::EmploymentEnd),
        f.staff_id,
        f.customer_id,
        limit,
        f.pattern,
        a_at,
        a_kind,
        a_key,
        a_day,
        a.may(NoteKind::AppointmentNote),
        a.may(NoteKind::EvaluationNote),
        a.may(NoteKind::StaffNote),
        a.may(NoteKind::PackageExtension),
        a.discount_on_visit,
        a.discount_on_sale,
        a.may(NoteKind::AppointmentReason),
    )
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

/// Same purpose and ids `operations` encrypts a session note under (`operations::service::visit_aad`).
fn visit_note_aad(tenant: Uuid, visit: Uuid) -> Vec<u8> {
    kernel::crypto::record_aad("visit.note", &[tenant, visit])
}

/// The note's text: plain from SQL, or decrypted with its writer's associated data. The error
/// names the record only, never the text.
fn text_of(
    row: &NoteRow,
    kind: NoteKind,
    tenant: Uuid,
    cipher: &FieldCipher,
) -> Result<String, AppError> {
    let Some(ct) = row.ct.as_deref() else {
        return Ok(row.body.clone().unwrap_or_default());
    };
    let aad = match (kind, row.visit_id, row.customer_id) {
        (NoteKind::VisitNote, Some(visit), _) => visit_note_aad(tenant, visit),
        (NoteKind::HealthNote, _, Some(customer)) => crm::cipher::customer_aad(tenant, customer),
        _ => {
            return Err(AppError::Internal(format!(
                "note {} has no key context",
                row.key
            )));
        }
    };
    cipher
        .decrypt(ct, &aad)
        .map_err(|e| AppError::Internal(format!("note {} of kind {}: {e}", row.key, kind.as_str())))
}

fn entry(row: NoteRow, kind: NoteKind, text: String) -> NoteEntry {
    NoteEntry {
        kind,
        at: row.at,
        business_date: row.day,
        text,
        customer_id: row.customer_id,
        customer_name: row.customer_name,
        staff_id: row.staff_id,
        staff_name: row.staff_name,
        author_name: row.author_name,
        visit_id: row.visit_id,
        detail: row.detail,
        about_date: row.about,
    }
}

/// `GET /notes` (`report.read`, then each kind under its own rule; see [`Allowed`]).
pub async fn list(
    pool: &PgPool,
    cipher: &FieldCipher,
    actor: &Actor,
    query: &NotesQuery,
) -> Result<NotesPage, AppError> {
    actor.require(Permission::ReportRead)?;
    let limit = cursor::limit(query.limit)?;
    let search = text_filter(query.q.as_deref())?;
    let mut after: After = match query.cursor.as_deref() {
        Some(raw) => {
            let p = cursor::decode(raw, 4)?;
            if NoteKind::parse(&p[2]).is_none() {
                return Err(field("cursor", "invalid"));
            }
            Some((
                cursor::date(&p[0])?,
                cursor::instant(&p[1])?,
                p[2].clone(),
                p[3].clone(),
            ))
        }
        None => None,
    };
    let allowed = Allowed::of(actor);
    let kinds: Vec<NoteKind> = NoteKind::ALL
        .into_iter()
        .filter(|k| allowed.may(*k))
        .collect();

    let mut tx = TenantTx::begin(pool, actor.tenant_id).await?;
    let profile = kernel::tenant::profile(&mut tx).await?;
    let (from, to) = range(query.from, query.to, profile.business_date.as_naive())?;
    let tz = profile.timezone.name();
    let tenant = tx.tenant().as_uuid();
    let filters = Filters {
        from,
        to,
        allowed: allowed.only(query.kind),
        staff_id: query.staff_id,
        customer_id: query.customer_id,
        pattern: search.as_ref().map(|(_, p)| p.as_str()),
    };
    let page = usize::try_from(limit).unwrap_or(usize::MAX);
    // Without a search every fetched row is a hit: one round of limit + 1. With one, encrypted
    // rows still need a look after decryption, so scan in batches up to the cap.
    let batch = if search.is_some() {
        SEARCH_BATCH.max(limit + 1)
    } else {
        limit + 1
    };

    let mut items: Vec<NoteEntry> = Vec::with_capacity(page);
    let mut scanned = 0usize;
    let mut next_cursor = None;
    let mut scan_capped = false;
    'scan: loop {
        let rows = fetch(&mut tx, tz, &filters, &after, batch).await?;
        let exhausted = i64::try_from(rows.len()).unwrap_or(i64::MAX) < batch;
        let count = rows.len();
        for (i, row) in rows.into_iter().enumerate() {
            let kind = NoteKind::parse(&row.kind)
                .ok_or_else(|| AppError::Internal(format!("note kind {}", row.kind)))?;
            let position = (row.day, row.at, row.kind.clone(), row.key.clone());
            if items.len() == page {
                // A row past a full page: there is more.
                next_cursor = encode(after.take());
                break 'scan;
            }
            scanned += 1;
            let text = text_of(&row, kind, tenant, cipher)?;
            let hit = match (&search, row.ct.is_some()) {
                (Some((key, _)), true) => row.name_hit || search_key(&text).contains(key.as_str()),
                _ => true,
            };
            if hit {
                items.push(entry(row, kind, text));
            }
            after = Some(position);
            let last_of_batch = i + 1 == count;
            if search.is_some()
                && scanned >= SCAN_CAP
                && items.len() < page
                && !(last_of_batch && exhausted)
            {
                scan_capped = true;
                next_cursor = encode(after.take());
                break 'scan;
            }
        }
        if exhausted {
            break;
        }
    }
    tx.commit().await?;
    Ok(NotesPage {
        from,
        to,
        items,
        kinds,
        next_cursor,
        scan_capped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    #[test]
    fn range_defaults_to_the_last_30_days_and_is_bounded() -> Result<(), AppError> {
        let today = day("2026-11-07");
        assert_eq!(range(None, None, today)?, (day("2026-10-09"), today));
        assert_eq!(
            range(Some(day("2026-01-01")), Some(day("2026-12-31")), today)?.0,
            day("2026-01-01")
        );
        assert!(range(Some(day("2026-11-08")), Some(day("2026-11-07")), today).is_err());
        assert!(range(Some(day("2025-01-01")), Some(day("2026-01-02")), today).is_err());
        Ok(())
    }

    #[test]
    fn search_text_is_folded_escaped_and_bounded() -> Result<(), AppError> {
        assert_eq!(text_filter(Some("  "))?, None);
        let (key, pattern) = text_filter(Some("Đau Lưng_%"))?.unwrap_or_default();
        assert_eq!(key, "dau lung_%");
        assert_eq!(pattern, r"%dau lung\_\%%");
        assert!(text_filter(Some("a")).is_err());
        assert!(text_filter(Some(&"x".repeat(101))).is_err());
        Ok(())
    }

    #[test]
    fn a_kind_filter_never_widens_what_the_caller_may_read() {
        let actor = |perms: &[Permission]| {
            Actor::new(
                kernel::UserId::new(),
                kernel::TenantId::from_uuid(Uuid::now_v7()),
                perms.iter().copied(),
                None,
            )
        };
        let reader = Allowed::of(&actor(&[Permission::CustomerRead]));
        assert!(
            !reader
                .only(Some(NoteKind::HealthNote))
                .may(NoteKind::HealthNote)
        );
        assert!(
            reader
                .only(Some(NoteKind::CustomerNote))
                .may(NoteKind::CustomerNote)
        );
        assert!(
            !reader
                .only(Some(NoteKind::CustomerNote))
                .may(NoteKind::FollowUp)
        );
        // Discount: each source keeps its own rule, and a filter on another kind drops both.
        let visits = Allowed::of(&actor(&[Permission::VisitRead]));
        assert!(
            visits.discount_on_visit && !visits.discount_on_sale && visits.may(NoteKind::Discount)
        );
        let sales = Allowed::of(&actor(&[Permission::SaleRead]));
        assert!(
            !sales.may(NoteKind::Discount),
            "sale.read without customer.read"
        );
        let narrowed = visits.only(Some(NoteKind::VisitNote));
        assert!(!narrowed.discount_on_visit && !narrowed.discount_on_sale);
        for (i, k) in NoteKind::ALL.into_iter().enumerate() {
            assert_eq!(NoteKind::parse(k.as_str()), Some(k));
            assert_eq!(k as usize, i, "ALL follows the declaration order");
        }
    }
}
