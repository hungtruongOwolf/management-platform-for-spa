//! Demo staff: the roster (five technicians, the manager, a technician who left mid-month last
//! month with her login kept, one who joined about four months ago), their pay terms and logins,
//! and a year of their books — attendance every day, daily reviews, bonuses and penalties, monthly
//! evaluations — then every ended month's payroll closed. All through the org/hr use cases
//! (validation, audit, period lock), months side by side.

use super::rng::Rng;
use super::run::{each, retry};
use super::{Window, fail, idempotency};
use anyhow::Context;
use chrono::{Duration, NaiveDate, NaiveTime};
use hr::adjustment::{AdjustmentInput, AdjustmentKind};
use hr::attendance::{AttendanceStatus, Entry};
use hr::evaluation::{CRITERIA, CriterionInput, EvaluationInput, Level};
use hr::month::Month;
use hr::review::ReviewEntry;
use kernel::{Actor, UserId};
use org::profile::{EndInput, PayInput, ProfileInput};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Demo technician login, linked to the first technician (self-service screens).
pub const TECHNICIAN_PHONE: &str = "0905000003";
/// Login of the technician who left last month (ADR-0014: she still sees her past months).
pub const FORMER_TECHNICIAN_PHONE: &str = "0905000004";

const TECHNICIAN: &str = "Kỹ thuật viên";
const MANAGER: &str = "Quản lý";

/// Which login a roster member signs in with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Login {
    Manager,
    Technician,
    FormerTechnician,
}

/// One person on the demo roster.
#[derive(Debug, Clone)]
pub struct Member {
    pub name: &'static str,
    pub technician: bool,
    pub login: Option<Login>,
    pub joined: NaiveDate,
    /// Last day of work (she left; ADR-0013 period ended with a reason).
    pub left: Option<NaiveDate>,
    /// (base salary, commission bp, loyalty bonus per month, meal allowance)
    pub pay: (i64, i32, i64, i64),
    /// Takes more days off than the others (payslip deductions, a weak evaluation).
    pub often_absent: bool,
}

impl Member {
    pub fn works_on(&self, day: NaiveDate) -> bool {
        self.joined <= day && self.left.is_none_or(|l| day <= l)
    }

    fn works_in(&self, month: Month) -> bool {
        self.joined <= month.last_day() && self.left.is_none_or(|l| l >= month.first_day())
    }
}

/// The roster, dated relative to `today`. Index 0 is the manager; the leaver is last.
pub fn roster(today: NaiveDate) -> Vec<Member> {
    let this_month = Month::of(today);
    let last_month = Month::of(this_month.first_day() - Duration::days(1));
    let mut four_ago = this_month;
    for _ in 0..4 {
        four_ago = Month::of(four_ago.first_day() - Duration::days(1));
    }
    let ago = |days: i64| today - Duration::days(days);
    let m = |name, technician, login, joined, pay| Member {
        name,
        technician,
        login,
        joined,
        left: None,
        pay,
        often_absent: false,
    };
    vec![
        m(
            super::data::MANAGER_NAME,
            false,
            Some(Login::Manager),
            ago(1_400),
            (7_000_000, 300, 0, 500_000),
        ),
        m(
            "Nguyễn Thị Lan",
            true,
            Some(Login::Technician),
            ago(620),
            (5_000_000, 500, 500_000, 300_000),
        ),
        m(
            "Trần Thị Thu",
            true,
            None,
            ago(900),
            (5_000_000, 700, 500_000, 300_000),
        ),
        // Joined on the 20th four months ago: a part month (base pro rata, no loyalty bonus under
        // 15 days, ADR-0019 D4), then full months.
        m(
            "Lê Thị Hoa",
            true,
            None,
            four_ago.first_day() + Duration::days(19),
            (4_500_000, 1_000, 500_000, 300_000),
        ),
        Member {
            often_absent: true,
            ..m(
                "Phạm Văn Nam",
                true,
                None,
                ago(1_250),
                (6_050_000, 500, 500_000, 400_000),
            )
        },
        m(
            "Võ Thị Hằng",
            true,
            None,
            ago(380),
            (5_000_000, 700, 0, 300_000),
        ),
        // Left on the 18th of last month: 18 days ≥ 15, so that part month still earns the
        // loyalty bonus; her login now sees only her past months.
        Member {
            left: Some(
                (last_month.first_day() + Duration::days(17)).min(today - Duration::days(1)),
            ),
            ..m(
                "Đỗ Thị Ngân",
                true,
                Some(Login::FormerTechnician),
                ago(700),
                (5_000_000, 500, 500_000, 300_000),
            )
        },
    ]
}

/// One attendance cell of the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub status: AttendanceStatus,
    pub check_in: Option<NaiveTime>,
    pub check_out: Option<NaiveTime>,
}

/// Who is at work each day: `cells[member][day offset from the window start]`.
pub struct Attendance {
    start: NaiveDate,
    cells: Vec<Vec<Option<Cell>>>,
}

fn time(h: u32, m: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h, m, 0)
}

impl Attendance {
    /// Every working day of every member from the window start to today (today: checked in,
    /// not out yet).
    pub fn plan(window: Window, roster: &[Member], rng: &mut Rng) -> Self {
        let days = usize::try_from(window.days() + 1).unwrap_or(0);
        let cells = roster
            .iter()
            .map(|member| {
                (0..days)
                    .map(|offset| {
                        let day = window.start + Duration::days(i64::try_from(offset).ok()?);
                        if !member.works_on(day) {
                            return None;
                        }
                        let roll = rng.below(100);
                        let absent = if member.often_absent { 12 } else { 6 };
                        let status = match roll {
                            r if r >= absent => AttendanceStatus::Present,
                            0..3 => AttendanceStatus::LeaveP,
                            3..5 => AttendanceStatus::LeaveDx,
                            _ => AttendanceStatus::AbsentK,
                        };
                        let late = rng.chance(if member.often_absent { 30 } else { 12 });
                        let minute = u32::try_from(rng.below(20)).unwrap_or(0);
                        let out = u32::try_from(rng.below(60)).unwrap_or(0);
                        let present = status == AttendanceStatus::Present;
                        Some(Cell {
                            status,
                            check_in: if !present {
                                None
                            } else if late {
                                time(9, 1 + minute)
                            } else {
                                time(8, 41 + minute)
                            },
                            check_out: (present && day < window.today)
                                .then(|| time(18, out))
                                .flatten(),
                        })
                    })
                    .collect()
            })
            .collect();
        Self {
            start: window.start,
            cells,
        }
    }

    pub fn cell(&self, member: usize, day: NaiveDate) -> Option<Cell> {
        let offset = usize::try_from((day - self.start).num_days()).ok()?;
        self.cells.get(member)?.get(offset).copied().flatten()
    }

    /// Technicians at work on `day` (indices into the roster); every employed technician when
    /// nobody is (never leaves a visit without a technician).
    pub fn technicians_on(&self, roster: &[Member], day: NaiveDate) -> Vec<usize> {
        let present: Vec<usize> = (0..roster.len())
            .filter(|&m| roster[m].technician)
            .filter(|&m| {
                self.cell(m, day)
                    .is_some_and(|c| c.status == AttendanceStatus::Present)
            })
            .collect();
        if present.is_empty() {
            (0..roster.len())
                .filter(|&m| roster[m].technician && roster[m].works_on(day))
                .collect()
        } else {
            present
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StaffReport {
    pub roster: usize,
    pub staff_added: usize,
    pub attendance_days: usize,
    pub leave_days: usize,
    pub late_days: usize,
    pub daily_reviews: usize,
    pub evaluations: usize,
    pub weak_evaluations: usize,
    pub adjustments: usize,
    pub payroll_months_closed: usize,
    pub payslips: usize,
    pub technician_login: bool,
    pub former_staff_with_login: bool,
}

/// Staff ids by roster index, after [`setup`].
pub struct Staff {
    pub ids: Vec<Uuid>,
}

pub struct Logins {
    pub manager: UserId,
    pub technician: UserId,
    pub former: UserId,
}

/// Adds the roster members that are missing, then sets their start date, shift, pay terms and
/// login link as the owner.
pub async fn setup(
    owner_pool: &PgPool,
    app: &PgPool,
    owner: &Actor,
    slug: &str,
    roster: &[Member],
    logins: &Logins,
    report: &mut StaffReport,
) -> anyhow::Result<Staff> {
    let names = |list: Vec<org::profile::ProfileView>| -> HashMap<String, Uuid> {
        list.into_iter()
            .map(|v| (v.profile.full_name, v.profile.id))
            .collect()
    };
    let existing = names(
        org::profile::list(app, owner, None)
            .await
            .map_err(fail("read the roster"))?,
    );
    for m in roster {
        if !existing.contains_key(m.name) {
            let position = if m.technician { TECHNICIAN } else { MANAGER };
            org::provisioning::add_staff(owner_pool, slug, m.name, position)
                .await
                .map_err(fail("add staff"))?;
            report.staff_added += 1;
        }
    }
    let all = names(
        org::profile::list(app, owner, None)
            .await
            .map_err(fail("read the roster"))?,
    );
    let mut ids = Vec::with_capacity(roster.len());
    for m in roster {
        let id = *all.get(m.name).context("roster member vanished")?;
        let view = org::profile::get(app, owner, id)
            .await
            .map_err(fail("read a staff profile"))?;
        let pay_version = view.pay.map_or(0, |t| t.version);
        let p = view.profile;
        let updated = org::profile::update(
            app,
            owner,
            id,
            p.version,
            &ProfileInput {
                full_name: p.full_name,
                position: p.position,
                phone: p.phone.map(|ph| ph.as_str().to_owned()),
                joined_on: Some(m.joined),
                shift_start: time(9, 0),
                note: p.note,
            },
        )
        .await
        .map_err(fail("update a staff profile"))?;
        let login = match m.login {
            Some(Login::Manager) => Some(logins.manager),
            Some(Login::Technician) => Some(logins.technician),
            Some(Login::FormerTechnician) => Some(logins.former),
            None => None,
        };
        if let Some(user) = login
            && updated.profile.user_id != Some(user.as_uuid())
        {
            org::profile::set_account(
                app,
                owner,
                id,
                updated.profile.version,
                Some(user.as_uuid()),
            )
            .await
            .map_err(fail("link a staff login"))?;
        }
        let (base, rate, loyalty, meal) = m.pay;
        org::profile::set_pay(
            app,
            owner,
            id,
            pay_version,
            &PayInput {
                base_salary: base,
                commission_rate_bp: rate,
                loyalty_bonus_monthly: loyalty,
                meal_allowance: meal,
            },
        )
        .await
        .map_err(fail("set pay terms"))?;
        ids.push(id);
    }
    report.roster = ids.len();
    report.technician_login = true;
    Ok(Staff { ids })
}

/// The months of the window, oldest first; all but the last (this month) have ended.
fn months(window: Window) -> Vec<Month> {
    let mut out = vec![Month::of(window.start)];
    while let Some(&m) = out.last() {
        if m >= Month::of(window.today) {
            break;
        }
        out.push(m.next());
    }
    out
}

/// What the staff books need from the rest of the seed.
pub struct Books<'a> {
    pub app: &'a PgPool,
    pub owner: &'a Actor,
    pub manager: &'a Actor,
    pub window: Window,
    pub roster: &'a [Member],
    pub staff: &'a Staff,
    pub attendance: &'a Attendance,
    pub workers: usize,
}

impl Books<'_> {
    /// The whole year of staff records, then the leaver's end, then every ended month closed.
    pub async fn write(&self, rng: &mut Rng, report: &mut StaffReport) -> anyhow::Result<()> {
        self.attendance(report).await?;
        report.daily_reviews = self.reviews(rng).await?;
        report.adjustments = self.adjustments(rng).await?;
        let (evaluations, weak) = self.evaluations(rng).await?;
        report.evaluations = evaluations;
        report.weak_evaluations = weak;
        report.former_staff_with_login = self.end_leaver().await?;
        let (closed, payslips) = self.close_payroll().await?;
        report.payroll_months_closed = closed;
        report.payslips = payslips;
        Ok(())
    }

    /// Every cell of the plan, the manager's by the owner (decision C), in batches of at most
    /// [`hr::attendance::MAX_ENTRIES`], months side by side.
    async fn attendance(&self, report: &mut StaffReport) -> anyhow::Result<()> {
        let mut batches: Vec<(bool, Vec<Entry>)> = Vec::new();
        for month in months(self.window) {
            let (mut own, mut team) = (Vec::new(), Vec::new());
            let mut day = month.first_day().max(self.window.start);
            while day <= month.last_day().min(self.window.today) {
                for (m, member) in self.roster.iter().enumerate() {
                    let Some(cell) = self.attendance.cell(m, day) else {
                        continue;
                    };
                    report.leave_days += usize::from(cell.status != AttendanceStatus::Present);
                    report.late_days +=
                        usize::from(cell.check_in.is_some_and(|t| t > time(9, 0).unwrap_or(t)));
                    let entry = Entry {
                        staff_id: self.staff.ids[m],
                        date: day,
                        status: Some(cell.status),
                        check_in: cell.check_in,
                        check_out: cell.check_out,
                        note: match cell.status {
                            AttendanceStatus::LeaveP => Some("Việc gia đình, đã báo trước".into()),
                            AttendanceStatus::LeaveDx => Some("Ốm, sẽ bổ sung giấy khám".into()),
                            AttendanceStatus::AbsentK => Some("Không báo trước".into()),
                            AttendanceStatus::Present => None,
                        },
                    };
                    if member.technician {
                        team.push(entry);
                    } else {
                        own.push(entry);
                    }
                }
                day += Duration::days(1);
            }
            for (by_owner, list) in [(false, team), (true, own)] {
                for chunk in list.chunks(hr::attendance::MAX_ENTRIES) {
                    batches.push((by_owner, chunk.to_vec()));
                }
            }
        }
        let recorded = each(batches, self.workers, |(by_owner, batch)| async move {
            let actor = if by_owner { self.owner } else { self.manager };
            let days = retry(|| hr::attendance::record(self.app, actor, &batch))
                .await
                .map_err(fail("record attendance"))?;
            Ok(days.len())
        })
        .await?;
        report.attendance_days = recorded.iter().sum();
        Ok(())
    }

    /// Compliments and complaints on about a fifth of the technicians' working days.
    async fn reviews(&self, rng: &mut Rng) -> anyhow::Result<usize> {
        let mut batches: Vec<Vec<ReviewEntry>> = Vec::new();
        for month in months(self.window) {
            let mut list = Vec::new();
            let mut day = month.first_day().max(self.window.start);
            while day < month.last_day().min(self.window.today) + Duration::days(1) {
                for (m, member) in self.roster.iter().enumerate() {
                    let at_work = self
                        .attendance
                        .cell(m, day)
                        .is_some_and(|c| c.status == AttendanceStatus::Present);
                    if !member.technician || !at_work {
                        continue;
                    }
                    let compliments = if rng.chance(20) { 1 + rng.below(2) } else { 0 };
                    let complaints =
                        usize::from(rng.chance(if member.often_absent { 8 } else { 3 }));
                    if compliments + complaints > 0 {
                        list.push(ReviewEntry {
                            staff_id: self.staff.ids[m],
                            date: day,
                            compliments: u32::try_from(compliments).unwrap_or(0),
                            complaints: u32::try_from(complaints).unwrap_or(0),
                            note: rng.pick(REVIEW_NOTES).map(|n| (*n).to_owned()),
                        });
                    }
                }
                day += Duration::days(1);
            }
            for chunk in list.chunks(hr::review::MAX_ENTRIES) {
                batches.push(chunk.to_vec());
            }
        }
        let recorded = each(batches, self.workers, |batch| async move {
            let rows = retry(|| hr::review::record(self.app, self.manager, &batch))
                .await
                .map_err(fail("record daily reviews"))?;
            Ok(rows.len())
        })
        .await?;
        Ok(recorded.iter().sum())
    }

    /// One to three bonuses or penalties a month, in money or points.
    async fn adjustments(&self, rng: &mut Rng) -> anyhow::Result<usize> {
        let mut items = Vec::new();
        for month in months(self.window) {
            for _ in 0..1 + rng.below(3) {
                let m = 1 + rng.below(self.roster.len() - 1);
                let day =
                    month.first_day() + Duration::days(i64::try_from(rng.below(28)).unwrap_or(0));
                if day > self.window.today
                    || day < self.window.start
                    || !self.roster[m].works_on(day)
                {
                    continue;
                }
                let &(kind, amount, points, reason) =
                    rng.pick(ADJUSTMENTS).context("adjustments")?;
                items.push(AdjustmentInput {
                    staff_id: self.staff.ids[m],
                    kind,
                    amount,
                    points,
                    reason: reason.to_owned(),
                    occurred_on: Some(day),
                    corrects_month: None,
                });
            }
        }
        let done = each(items, self.workers, |input| async move {
            let (key, hash) = idempotency();
            retry(|| {
                hr::adjustment::record(self.app, self.manager, key, hash, &input, |a| {
                    serde_json::to_value(a).map_err(|e| kernel::AppError::Internal(e.to_string()))
                })
            })
            .await
            .map_err(fail("record an adjustment"))?;
            Ok(())
        })
        .await?;
        Ok(done.len())
    }

    /// Every ended month of every member: technicians by the manager, the manager by the owner.
    /// Mostly Mức 1–2; the often-absent technician falls to Mức 3 / Chưa đạt every third month
    /// (a coaching plan is then required).
    async fn evaluations(&self, rng: &mut Rng) -> anyhow::Result<(usize, usize)> {
        use Level::{Fail, M1, M2, M3};
        let ended: Vec<Month> = months(self.window)
            .into_iter()
            .filter(|m| m.last_day() < self.window.today)
            .collect();
        let mut cards = Vec::new();
        for (k, month) in ended.iter().enumerate() {
            for (m, member) in self.roster.iter().enumerate() {
                if !member.works_in(*month) {
                    continue;
                }
                let card: [Level; 6] = if member.often_absent && k % 3 == 0 {
                    [M3, M2, M3, Fail, M2, M3]
                } else {
                    std::array::from_fn(|_| if rng.chance(65) { M1 } else { M2 })
                };
                cards.push((m, *month, card));
            }
        }
        let weak = cards.iter().filter(|(_, _, c)| c.contains(&Fail)).count();
        let done = each(cards, self.workers, |(m, month, card)| async move {
            let criteria = CRITERIA
                .iter()
                .zip(card)
                .map(|(c, level)| CriterionInput {
                    key: c.key.to_owned(),
                    // Typed at the sheet's anchor of each level (decision D: points, not levels).
                    points: Some(
                        c.max
                            * match level {
                                M1 => 95,
                                M2 => 80,
                                M3 => 60,
                                Fail => 30,
                            }
                            / 100,
                    ),
                })
                .collect::<Vec<_>>();
            let weak = card.contains(&Fail);
            let input = EvaluationInput {
                staff_id: self.staff.ids[m],
                month,
                criteria,
                serious_issue: false,
                coaching_plan: weak.then(|| {
                    "Kèm lại quy trình body đá nóng 2 buổi/tuần; nhắc báo nghỉ trước 3 ngày."
                        .to_owned()
                }),
                comment: Some(if weak {
                    "Nghỉ nhiều trong tháng, kỹ thuật cần kèm thêm.".to_owned()
                } else {
                    "Làm tốt, khách hài lòng.".to_owned()
                }),
            };
            let actor = if self.roster[m].technician {
                self.manager
            } else {
                self.owner
            };
            retry(|| hr::evaluation::record(self.app, actor, &input))
                .await
                .map_err(fail("record an evaluation"))?;
            Ok(())
        })
        .await?;
        Ok((done.len(), weak))
    }

    /// The leaver's employment ends (reason recorded, login kept: ADR-0014).
    async fn end_leaver(&self) -> anyhow::Result<bool> {
        let Some((m, left)) = self
            .roster
            .iter()
            .enumerate()
            .find_map(|(m, member)| member.left.map(|l| (m, l)))
        else {
            return Ok(false);
        };
        let id = self.staff.ids[m];
        let view = org::profile::get(self.app, self.owner, id)
            .await
            .map_err(fail("read the leaver"))?;
        org::profile::end(
            self.app,
            self.owner,
            id,
            view.profile.version,
            &EndInput {
                ended_on: left,
                reason: "Chuyển về quê, nghỉ giữa tháng".to_owned(),
                lock_login: false,
            },
        )
        .await
        .map_err(fail("end the leaver's employment"))?;
        Ok(self.roster[m].login.is_some())
    }

    /// Closes every ended month, oldest first (frozen payslips; the month's books lock).
    async fn close_payroll(&self) -> anyhow::Result<(usize, usize)> {
        let mut closed = 0;
        let mut payslips = 0;
        for month in months(self.window) {
            if month.last_day() >= self.window.today {
                continue;
            }
            let (key, hash) = idempotency();
            let input = hr::payroll::CloseInput {
                month,
                allow_incomplete: false,
            };
            let (_, body) = retry(|| {
                hr::payroll::close(self.app, self.owner, key, hash, &input, |m| {
                    serde_json::to_value(m).map_err(|e| kernel::AppError::Internal(e.to_string()))
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("close payroll {month}: {} ({e:?})", e.code()))?;
            payslips += body["payslips"].as_array().map_or(0, Vec::len);
            closed += 1;
        }
        Ok((closed, payslips))
    }
}

const REVIEW_NOTES: &[&str] = &[
    "Khách khen tay nghề, hẹn quay lại",
    "Khách yêu cầu đích danh",
    "Khách phàn nàn lực tay hơi mạnh",
    "Tư vấn liệu trình tốt",
];

type Adjustment = (AdjustmentKind, Option<i64>, Option<u32>, &'static str);

const ADJUSTMENTS: &[Adjustment] = &[
    (
        AdjustmentKind::Bonus,
        Some(200_000),
        None,
        "Khách khen đích danh 3 lần",
    ),
    (
        AdjustmentKind::Bonus,
        Some(100_000),
        None,
        "Giới thiệu khách mới mua gói",
    ),
    (
        AdjustmentKind::Bonus,
        None,
        Some(5),
        "Chủ động học liệu trình mới",
    ),
    (
        AdjustmentKind::Penalty,
        Some(50_000),
        None,
        "Đi muộn nhiều lần không báo",
    ),
    (
        AdjustmentKind::Penalty,
        None,
        Some(2),
        "Quên vệ sinh giường sau ca",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap_or_default()
    }

    #[test]
    fn roster_has_a_mid_month_leaver_and_a_recent_joiner() {
        let roster = roster(d("2026-11-09"));
        assert_eq!(roster.iter().filter(|m| m.technician).count(), 6);
        let leaver = roster.iter().find(|m| m.left.is_some());
        assert_eq!(leaver.and_then(|m| m.left), Some(d("2026-10-18")));
        assert_eq!(leaver.and_then(|m| m.login), Some(Login::FormerTechnician));
        assert!(roster.iter().any(|m| m.joined == d("2026-07-20")));
    }

    #[test]
    fn attendance_covers_every_working_day_and_nothing_else() {
        let today = d("2026-11-09");
        let w = Window::new(today, 12);
        let roster = roster(today);
        let plan = Attendance::plan(w, &roster, &mut Rng::new(3));
        let mut day = w.start;
        while day <= today {
            for (m, member) in roster.iter().enumerate() {
                assert_eq!(plan.cell(m, day).is_some(), member.works_on(day), "{day}");
            }
            assert!(!plan.technicians_on(&roster, day).is_empty());
            day += Duration::days(1);
        }
        let leaver = roster.len() - 1;
        assert!(plan.cell(leaver, d("2026-10-19")).is_none());
        assert!(
            (0..roster.len()).all(|m| plan.cell(m, today).is_none_or(|c| c.check_out.is_none())),
            "nobody has left yet today"
        );
    }

    #[test]
    fn months_run_from_the_window_start_to_this_month() {
        let w = Window::new(d("2026-11-09"), 12);
        let m = months(w);
        assert_eq!(m.len(), 12);
        assert_eq!(m.first().map(|m| m.first_day()), Some(d("2025-12-01")));
        assert_eq!(m.last().map(|m| m.first_day()), Some(d("2026-11-01")));
    }
}
