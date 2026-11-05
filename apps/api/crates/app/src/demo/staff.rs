//! Demo staff management: start dates, pay terms, a technician login linked to its profile, and
//! attendance, daily reviews, evaluations and bonuses/penalties from the first day of last month
//! to yesterday — all through the hr/org use cases (validation, audit, period lock).
//! Last month stays open so the EM can try the close.

use super::{fail, idempotency, member_actor};
use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use hr::adjustment::{AdjustmentInput, AdjustmentKind};
use hr::attendance::{AttendanceStatus, Entry};
use hr::evaluation::{CRITERIA, CriterionInput, EvaluationInput, Level};
use hr::month::Month;
use hr::review::ReviewEntry;
use kernel::{Actor, TenantId};
use org::profile::{PayInput, ProfileInput};
use sqlx::PgPool;
use uuid::Uuid;

use super::rng::Rng;

/// Demo technician login, linked to the first technician (self-service screens).
pub const TECHNICIAN_PHONE: &str = "0905000003";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StaffDemoReport {
    pub attendance_days: usize,
    pub daily_reviews: usize,
    pub evaluations: usize,
    pub adjustments: usize,
    pub technician_login: bool,
}

pub struct StaffDemo<'a> {
    pub owner_pool: &'a PgPool,
    pub app: &'a PgPool,
    pub tenant: TenantId,
    pub slug: &'a str,
    pub password: &'a str,
    pub owner: &'a Actor,
    pub manager: &'a Actor,
    /// Technicians in `data::TECHNICIANS` order, then the manager's staff record.
    pub technicians: &'a [Uuid],
    pub manager_staff: Uuid,
    pub today: NaiveDate,
}

/// (base salary, commission bp, loyalty per month, meal allowance, joined on)
type Terms = (i64, i32, i64, i64, &'static str);

const TERMS: &[Terms] = &[
    (5_000_000, 500, 500_000, 300_000, "2025-03-01"),
    (5_500_000, 700, 500_000, 300_000, "2024-06-10"),
    (4_500_000, 1000, 0, 300_000, "2026-02-16"),
    (6_050_000, 500, 500_000, 400_000, "2023-09-01"),
    (5_000_000, 700, 0, 300_000, "2025-11-03"),
];
const MANAGER_TERMS: Terms = (7_000_000, 300, 0, 500_000, "2023-01-02");

fn date(s: &str) -> anyhow::Result<NaiveDate> {
    s.parse().context("demo date")
}

fn time(h: u32, m: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h, m, 0)
}

impl StaffDemo<'_> {
    pub async fn run(&self, rng: &mut Rng) -> anyhow::Result<StaffDemoReport> {
        let mut report = StaffDemoReport::default();
        let tech_user = identity::provisioning::add_user(
            self.owner_pool,
            identity::provisioning::NewUser {
                tenant_slug: self.slug,
                phone: TECHNICIAN_PHONE,
                full_name: super::data::TECHNICIANS[0],
                password: self.password,
                role: identity::SystemRole::Technician,
            },
        )
        .await
        .map_err(fail("add the demo technician"))?;
        // Make sure the login works in this spa before linking it.
        member_actor(self.app, tech_user, self.tenant).await?;
        report.technician_login = true;

        let everyone: Vec<(Uuid, Terms)> = self
            .technicians
            .iter()
            .copied()
            .zip(TERMS.iter().copied())
            .chain([(self.manager_staff, MANAGER_TERMS)])
            .collect();
        for (i, (staff, (base, rate, loyalty, meal, joined))) in everyone.iter().enumerate() {
            let view = org::profile::get(self.app, self.owner, *staff)
                .await
                .map_err(fail("read a staff profile"))?;
            let p = view.profile;
            org::profile::update(
                self.app,
                self.owner,
                *staff,
                p.version,
                &ProfileInput {
                    full_name: p.full_name,
                    position: p.position,
                    phone: p.phone.map(|ph| ph.as_str().to_owned()),
                    status: p.status,
                    joined_on: Some(date(joined)?),
                    left_on: None,
                    user_id: (i == 0).then_some(tech_user.as_uuid()),
                    shift_start: time(9, 0),
                },
            )
            .await
            .map_err(fail("update a staff profile"))?;
            org::profile::set_pay(
                self.app,
                self.owner,
                *staff,
                &PayInput {
                    base_salary: *base,
                    commission_rate_bp: *rate,
                    loyalty_bonus_monthly: *loyalty,
                    meal_allowance: *meal,
                },
            )
            .await
            .map_err(fail("set pay terms"))?;
        }

        let this_month = Month::of(self.today);
        let last_month = Month::of(this_month.first_day() - Duration::days(1));
        let staff: Vec<Uuid> = everyone.iter().map(|(id, _)| *id).collect();
        let mut day = last_month.first_day();
        while day < self.today {
            let joined = |i: usize| everyone[i].1.4;
            let mut entries = Vec::new();
            let mut reviews = Vec::new();
            for (i, id) in staff.iter().enumerate() {
                if day < date(joined(i))? {
                    continue;
                }
                entries.push(self.attendance(rng, *id, i, day));
                if i < self.technicians.len() {
                    let compliments = if rng.chance(25) { 1 + rng.below(2) } else { 0 };
                    let complaints = usize::from(rng.chance(6));
                    if compliments + complaints > 0 {
                        reviews.push(ReviewEntry {
                            staff_id: *id,
                            date: day,
                            compliments: u32::try_from(compliments).unwrap_or(0),
                            complaints: u32::try_from(complaints).unwrap_or(0),
                            note: rng.pick(REVIEW_NOTES).map(|n| (*n).to_owned()),
                        });
                    }
                }
            }
            if !entries.is_empty() {
                report.attendance_days += hr::attendance::record(self.app, self.manager, &entries)
                    .await
                    .map_err(fail("record attendance"))?
                    .len();
            }
            if !reviews.is_empty() {
                report.daily_reviews += hr::review::record(self.app, self.manager, &reviews)
                    .await
                    .map_err(fail("record daily reviews"))?
                    .len();
            }
            day += Duration::days(1);
        }

        report.evaluations = self.evaluate(last_month).await?;
        report.adjustments = self.adjust(last_month).await?;
        Ok(report)
    }

    /// Mostly on time at 08:45–09:05; some late; the odd P / ĐX / K. Technician #3 (index 3)
    /// takes four days off last month so the payslip shows a deduction.
    fn attendance(&self, rng: &mut Rng, staff: Uuid, i: usize, day: NaiveDate) -> Entry {
        let last_month_day = day < Month::of(self.today).first_day();
        let forced = (i == 3 && last_month_day).then(|| match day.day() {
            6 => Some(AttendanceStatus::LeaveP),
            13 => Some(AttendanceStatus::LeaveDx),
            20 | 21 => Some(AttendanceStatus::AbsentK),
            _ => None,
        });
        let roll = rng.below(100);
        let status = forced.flatten().unwrap_or(match roll {
            0..=2 => AttendanceStatus::LeaveP,
            3..=4 => AttendanceStatus::LeaveDx,
            5 => AttendanceStatus::AbsentK,
            _ => AttendanceStatus::Present,
        });
        let present = status == AttendanceStatus::Present;
        let late = rng.chance(15);
        let minute = u32::try_from(rng.below(20)).unwrap_or(0);
        let check_in = if late {
            time(9, 1 + minute)
        } else {
            time(8, 45 + minute)
        };
        let out = u32::try_from(rng.below(60)).unwrap_or(0);
        Entry {
            staff_id: staff,
            date: day,
            status: Some(status),
            check_in: present.then_some(check_in).flatten(),
            check_out: present.then(|| time(18, out)).flatten(),
            note: match status {
                AttendanceStatus::LeaveP => Some("Việc gia đình, đã báo trước".to_owned()),
                AttendanceStatus::LeaveDx => Some("Ốm, sẽ bổ sung giấy khám".to_owned()),
                AttendanceStatus::AbsentK => Some("Không báo trước".to_owned()),
                AttendanceStatus::Present => None,
            },
        }
    }

    async fn evaluate(&self, month: Month) -> anyhow::Result<usize> {
        use Level::{Fail, M1, M2, M3};
        let cards: [[Level; 6]; 5] = [
            [M1, M1, M1, M1, M1, M2],
            [M2, M1, M2, M2, M1, M2],
            [M1, M2, M1, M1, M2, M1],
            [M3, M2, M3, Fail, M2, M3],
            [M2, M2, M1, M2, M2, M2],
        ];
        let mut n = 0;
        for (staff, card) in self.technicians.iter().zip(cards) {
            let criteria = CRITERIA
                .iter()
                .zip(card)
                .map(|(c, level)| CriterionInput {
                    key: c.key.to_owned(),
                    level,
                    points: None,
                })
                .collect();
            let weak = card.contains(&Fail);
            hr::evaluation::record(
                self.app,
                self.manager,
                &EvaluationInput {
                    staff_id: *staff,
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
                },
            )
            .await
            .map_err(fail("record an evaluation"))?;
            n += 1;
        }
        Ok(n)
    }

    async fn adjust(&self, last_month: Month) -> anyhow::Result<usize> {
        let t = self.technicians;
        let yesterday = self.today - Duration::days(1);
        let mid_last = last_month.first_day() + Duration::days(14);
        let items = [
            (
                t[0],
                AdjustmentKind::Bonus,
                Some(200_000),
                None,
                "Khách khen đích danh 3 lần",
                mid_last,
            ),
            (
                t[3],
                AdjustmentKind::Penalty,
                Some(50_000),
                None,
                "Đi muộn 4 lần không báo",
                last_month.last_day(),
            ),
            (
                t[2],
                AdjustmentKind::Bonus,
                None,
                Some(5),
                "Chủ động học liệu trình mới",
                mid_last,
            ),
            (
                t[1],
                AdjustmentKind::Bonus,
                Some(100_000),
                None,
                "Giới thiệu khách mới mua gói",
                yesterday,
            ),
            (
                t[4],
                AdjustmentKind::Penalty,
                None,
                Some(2),
                "Quên vệ sinh giường sau ca",
                yesterday,
            ),
        ];
        let mut n = 0;
        for (staff, kind, amount, points, reason, on) in items {
            let (key, hash) = idempotency();
            hr::adjustment::record(
                self.app,
                self.manager,
                key,
                hash,
                &AdjustmentInput {
                    staff_id: staff,
                    kind,
                    amount,
                    points,
                    reason: reason.to_owned(),
                    occurred_on: Some(on),
                    corrects_month: None,
                },
                |a| serde_json::to_value(a).map_err(|e| kernel::AppError::Internal(e.to_string())),
            )
            .await
            .map_err(fail("record an adjustment"))?;
            n += 1;
        }
        Ok(n)
    }
}

const REVIEW_NOTES: &[&str] = &[
    "Khách khen tay nghề, hẹn quay lại",
    "Khách yêu cầu đích danh",
    "Khách phàn nàn lực tay hơi mạnh",
    "Tư vấn liệu trình tốt",
];
