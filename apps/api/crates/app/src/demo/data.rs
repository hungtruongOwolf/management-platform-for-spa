//! What the demo spa is made of: people, package plans and notes a real front desk would type.
//! Every name is invented; phone numbers come from a reserved block (see [`super::phone`]).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sex {
    Female,
    Male,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gender {
    Recorded(Sex),
    /// Not on the profile: the desk picks this price variant on each one-off line (AC4.2b).
    Unrecorded(Sex),
}

pub struct DemoCustomer {
    pub full_name: &'static str,
    pub gender: Gender,
    pub birth_year: Option<i16>,
    pub address: Option<&'static str>,
    pub source: Option<&'static str>,
    pub health_note: Option<&'static str>,
}

const fn c(
    full_name: &'static str,
    gender: Gender,
    birth_year: Option<i16>,
    address: Option<&'static str>,
    source: Option<&'static str>,
    health_note: Option<&'static str>,
) -> DemoCustomer {
    DemoCustomer {
        full_name,
        gender,
        birth_year,
        address,
        source,
        health_note,
    }
}

const F: Gender = Gender::Recorded(Sex::Female);
const M: Gender = Gender::Recorded(Sex::Male);

/// 40 customers. The first two are created by the paper-book import, not by the profile form.
pub const CUSTOMERS: &[DemoCustomer] = &[
    c(
        "Hoàng Thị Mai",
        F,
        Some(1968),
        Some("Phường 1, Đông Hà"),
        Some("Khách cũ từ sổ giấy"),
        Some("Huyết áp cao — tránh đá nóng"),
    ),
    c(
        "Phan Thị Thùy Dung",
        F,
        Some(1985),
        Some("Phường Đông Lương, Đông Hà"),
        Some("Khách cũ từ sổ giấy"),
        None,
    ),
    c(
        "Nguyễn Thị Lan Anh",
        F,
        Some(1990),
        Some("Phường 5, Đông Hà"),
        Some("Facebook"),
        Some("Đau vai gáy"),
    ),
    c(
        "Trần Thị Bích Ngọc",
        F,
        Some(1987),
        Some("Phường Đông Lễ, Đông Hà"),
        Some("Người quen giới thiệu"),
        None,
    ),
    c(
        "Lê Thị Hồng Nhung",
        F,
        Some(1995),
        None,
        Some("TikTok"),
        Some("Da nhạy cảm, dễ kích ứng mặt nạ"),
    ),
    c(
        "Phạm Thị Thu Trang",
        F,
        Some(1979),
        Some("Phường 3, Đông Hà"),
        Some("Người quen giới thiệu"),
        None,
    ),
    c(
        "Trần Văn Hùng",
        M,
        Some(1972),
        Some("Thị trấn Cam Lộ"),
        Some("Đi ngang qua"),
        Some("Thoát vị đĩa đệm L4-L5 — massage lưng nhẹ tay"),
    ),
    c(
        "Võ Thị Kim Oanh",
        F,
        Some(1983),
        Some("Phường Đông Thanh, Đông Hà"),
        Some("Zalo"),
        None,
    ),
    c(
        "Đặng Thị Thanh Hương",
        F,
        Some(1976),
        Some("Phường 2, Đông Hà"),
        Some("Facebook"),
        Some("Mất ngủ, hay đau đầu vùng thái dương"),
    ),
    c(
        "Nguyễn Văn Đức",
        M,
        Some(1980),
        Some("Phường Đông Giang, Đông Hà"),
        Some("Google Maps"),
        None,
    ),
    c(
        "Bùi Thị Ngọc Hân",
        F,
        Some(1998),
        None,
        Some("TikTok"),
        None,
    ),
    c(
        "Đỗ Thị Phương Thảo",
        F,
        Some(1992),
        Some("Phường 5, Đông Hà"),
        Some("Facebook"),
        Some("Đang mang thai 4 tháng — không bấm huyệt vùng bụng, tránh tinh dầu mạnh"),
    ),
    c(
        "Lê Quang Minh",
        M,
        Some(1988),
        Some("Phường 1, Đông Hà"),
        Some("Người quen giới thiệu"),
        Some("Đau vai gáy do ngồi máy tính nhiều"),
    ),
    c(
        "Hồ Thị Minh Châu",
        F,
        Some(1970),
        Some("Thị trấn Ái Tử, Triệu Phong"),
        Some("Người quen giới thiệu"),
        Some("Giãn tĩnh mạch chân — không day mạnh bắp chân"),
    ),
    c(
        "Ngô Thị Diễm My",
        F,
        Some(1996),
        None,
        Some("Facebook"),
        None,
    ),
    c(
        "Dương Thị Hải Yến",
        F,
        Some(1984),
        Some("Phường Đông Lương, Đông Hà"),
        Some("Zalo"),
        None,
    ),
    c(
        "Phạm Đình Khôi",
        M,
        Some(1965),
        Some("Phường 3, Đông Hà"),
        Some("Đi ngang qua"),
        Some("Tiểu đường type 2"),
    ),
    c(
        "Lý Thị Mỹ Linh",
        F,
        Some(1991),
        Some("Phường 2, Đông Hà"),
        Some("Facebook"),
        None,
    ),
    c(
        "Trương Thị Kim Liên",
        F,
        Some(1974),
        Some("Thị trấn Gio Linh"),
        Some("Người quen giới thiệu"),
        Some("Sau mổ vai phải (03/2026) — tránh xoay vai"),
    ),
    c(
        "Huỳnh Thị Như Quỳnh",
        F,
        Some(1999),
        None,
        Some("TikTok"),
        None,
    ),
    c(
        "Hoàng Văn Thắng",
        M,
        Some(1977),
        Some("Phường Đông Thanh, Đông Hà"),
        Some("Google Maps"),
        None,
    ),
    c(
        "Lê Thị Hoài Thương",
        F,
        Some(1993),
        Some("Phường 5, Đông Hà"),
        Some("Facebook"),
        Some("Viêm xoang"),
    ),
    c(
        "Nguyễn Thị Hạnh",
        F,
        Some(1962),
        Some("Phường 1, Đông Hà"),
        Some("Người quen giới thiệu"),
        Some("Hay chóng mặt khi xông hơi lâu — tối đa 10 phút"),
    ),
    c("Võ Minh Tuấn", M, Some(1986), None, Some("Zalo"), None),
    c(
        "Trần Thị Thu Hiền",
        F,
        Some(1981),
        Some("Phường Đông Lễ, Đông Hà"),
        Some("Facebook"),
        None,
    ),
    c(
        "Phạm Thị Ánh Tuyết",
        F,
        Some(1978),
        Some("Phường 4, Đông Hà"),
        Some("Người quen giới thiệu"),
        None,
    ),
    c(
        "Đặng Quốc Bảo",
        M,
        Some(1994),
        Some("Phường Đông Giang, Đông Hà"),
        Some("TikTok"),
        None,
    ),
    c(
        "Võ Thị Bảo Ngân",
        F,
        Some(1997),
        None,
        Some("Facebook"),
        Some("Dị ứng tinh dầu sả"),
    ),
    c(
        "Nguyễn Thị Cẩm Tú",
        F,
        Some(1989),
        Some("Phường 2, Đông Hà"),
        Some("Google Maps"),
        None,
    ),
    c(
        "Nguyễn Hữu Phước",
        M,
        Some(1969),
        Some("Thị trấn Cam Lộ"),
        Some("Đi ngang qua"),
        None,
    ),
    c(
        "Đinh Thị Lệ Hằng",
        F,
        Some(1982),
        Some("Phường 3, Đông Hà"),
        Some("Zalo"),
        None,
    ),
    c(
        "Mai Thị Xuân",
        F,
        Some(1966),
        Some("Phường Đông Lương, Đông Hà"),
        Some("Người quen giới thiệu"),
        Some("Đau khớp gối, đi lại khó — hỗ trợ lên xuống giường"),
    ),
    c(
        "Bùi Văn Lợi",
        M,
        Some(1975),
        None,
        Some("Người quen giới thiệu"),
        None,
    ),
    c(
        "Cao Thị Hoa",
        F,
        Some(1988),
        Some("Phường 1, Đông Hà"),
        Some("Facebook"),
        None,
    ),
    c(
        "Tôn Nữ Minh Thư",
        F,
        Some(2000),
        Some("Phường 5, Đông Hà"),
        Some("TikTok"),
        None,
    ),
    c(
        "Trần Đức Anh",
        Gender::Unrecorded(Sex::Male),
        Some(1995),
        None,
        Some("Đi ngang qua"),
        None,
    ),
    c(
        "Lương Thị Quế",
        F,
        Some(1971),
        Some("Thị trấn Ái Tử, Triệu Phong"),
        Some("Người quen giới thiệu"),
        None,
    ),
    c(
        "Phan Văn Tài",
        M,
        Some(1983),
        Some("Phường Đông Thanh, Đông Hà"),
        Some("Google Maps"),
        None,
    ),
    c(
        "Nguyễn Ngọc Hà Vy",
        Gender::Unrecorded(Sex::Female),
        Some(2001),
        None,
        Some("Facebook"),
        None,
    ),
    c(
        "Lê Văn Sơn",
        M,
        Some(1979),
        Some("Phường 4, Đông Hà"),
        Some("Zalo"),
        None,
    ),
];

/// Technicians (KTV) for the visit pickers.
pub const TECHNICIANS: &[&str] = &[
    "Nguyễn Thị Lan",
    "Trần Thị Thu",
    "Lê Thị Hoa",
    "Phạm Văn Nam",
    "Võ Thị Hằng",
];
pub const TECHNICIAN_POSITION: &str = "Kỹ thuật viên";

/// The demo manager: a login (role manager) and a staff member who closes sales (D9).
pub const MANAGER_NAME: &str = "Nguyễn Thị Thu Hà";
pub const MANAGER_PHONE: &str = "0905000002";
pub const MANAGER_POSITION: &str = "Quản lý";

pub struct DemoPlanItem {
    pub service: &'static str,
    pub sessions: i32,
    pub bonus_sessions: i32,
}

pub struct DemoPlan {
    pub name: &'static str,
    pub price: i64,
    pub validity_months: Option<i32>,
    pub gift_note: Option<&'static str>,
    pub items: &'static [DemoPlanItem],
}

const fn item(service: &'static str, sessions: i32, bonus_sessions: i32) -> DemoPlanItem {
    DemoPlanItem {
        service,
        sessions,
        bonus_sessions,
    }
}

/// Package plans built on the printed menu (`seed-catalog`); created when missing.
pub const PLANS: &[DemoPlan] = &[
    DemoPlan {
        name: "Gội đầu Thư Giãn 10 buổi + tặng 2",
        price: 790_000,
        validity_months: Some(6),
        gift_note: Some("Tặng 1 chai dầu gội thảo dược"),
        items: &[item("Thư Giãn", 10, 2)],
    },
    DemoPlan {
        name: "Cổ vai gáy An Kiên Lạc 10 buổi + tặng 1",
        price: 2_290_000,
        validity_months: Some(6),
        gift_note: None,
        items: &[item("An Kiên Lạc", 10, 1)],
    },
    DemoPlan {
        name: "Massage body Thân Tâm An Định 10 buổi + tặng 2",
        price: 2_690_000,
        validity_months: Some(12),
        gift_note: Some("Tặng 1 hũ muối ngâm chân"),
        items: &[item("Thân Tâm An Định", 10, 2)],
    },
    DemoPlan {
        name: "Xông chân thảo dược 10 buổi + tặng 2",
        price: 1_290_000,
        validity_months: None,
        gift_note: None,
        items: &[item("Xông chân thảo dược + Massage chân", 10, 2)],
    },
    DemoPlan {
        name: "Combo Gội Chuyên Sâu 5 + Cổ vai gáy 5",
        price: 1_690_000,
        validity_months: Some(3),
        gift_note: None,
        items: &[item("Chuyên Sâu", 5, 0), item("An Kiên Lạc", 5, 0)],
    },
];

/// Services customers without a package take one-off.
pub const ONE_OFF_SERVICES: &[&str] = &[
    "Cơ Bản – Dầu thảo dược",
    "Thư Giãn",
    "Chuyên Sâu",
    "Chuyên Sâu Đặc Biệt",
    "An Kiên Lạc",
    "Xông chân thảo dược + Massage chân",
    "Nằm giường đá muối Himalaya + Massage",
    "Thân Tâm An Định",
    "Bổ Thận An",
    "Đau đầu, mất ngủ, giảm stress",
    "Cạo gió mặt",
];

/// Small extras added to a visit and paid on the spot.
pub const ADD_ONS: &[&str] = &[
    "Mặt nạ thuốc bắc",
    "Tẩy tế bào chết da đầu",
    "Giác hơi + Đắp thuốc bắc",
    "Phòng xông hơi đá muối Himalaya",
];

/// Session notes (health data, encrypted like the customer's health note).
pub const VISIT_NOTES: &[&str] = &[
    "Vai trái còn căng, đã giảm lực tay",
    "Khách ngủ được trong buổi, phản hồi tốt",
    "Hơi đỏ da sau giác hơi, dặn khách uống nhiều nước",
    "Khách kêu đau đầu nhẹ sau xông, cho nghỉ 10 phút",
    "Đã dùng dầu không mùi theo yêu cầu",
];

pub const DISCOUNT_REASONS: &[&str] = &[
    "Khách quen",
    "Khuyến mãi tháng 11",
    "Khách giới thiệu bạn mới",
];
