//! The spa's printed menu (memory/wiki/product/service-catalog.md) as seed data.
//! Prices in đồng: `Flat(x)` for everyone, `Gendered(female, male)` where `None` = not offered.

pub enum SeedPrice {
    Flat(i64),
    Gendered(Option<i64>, Option<i64>),
    /// Price not printed on the menu yet: seeded inactive until the owner sets it.
    Unknown,
}

pub struct SeedService {
    pub name: &'static str,
    pub duration_min: Option<i32>,
    pub price: SeedPrice,
    pub recommended: bool,
    pub description: Option<&'static str>,
}

pub struct SeedCategory {
    pub name: &'static str,
    pub services: &'static [SeedService],
}

const fn s(
    name: &'static str,
    duration_min: Option<i32>,
    price: SeedPrice,
    recommended: bool,
    description: Option<&'static str>,
) -> SeedService {
    SeedService {
        name,
        duration_min,
        price,
        recommended,
        description,
    }
}

use SeedPrice::{Flat, Gendered, Unknown};

pub const MENU: &[SeedCategory] = &[
    SeedCategory {
        name: "Gội đầu dưỡng sinh",
        services: &[
            s(
                "Cơ Bản – Dầu thảo dược",
                Some(30),
                Flat(49_000),
                false,
                None,
            ),
            s("Cơ Bản – Dầu cặp", Some(30), Flat(39_000), false, None),
            s(
                "Thư Giãn",
                Some(45),
                Gendered(Some(79_000), Some(99_000)),
                false,
                Some("+ Cạo gió đầu, đá nóng"),
            ),
            s(
                "Chuyên Sâu",
                Some(60),
                Gendered(Some(119_000), Some(149_000)),
                false,
                Some("+ Massage mặt, đắp mặt nạ"),
            ),
            s(
                "Chuyên Sâu Đặc Biệt",
                Some(60),
                Gendered(Some(149_000), Some(199_000)),
                true,
                Some("+ Cạo gió mặt, mặt nạ thuốc bắc gia truyền"),
            ),
            s(
                "Chuyên Sâu Thăng Hoa",
                Some(90),
                Gendered(Some(199_000), Some(249_000)),
                false,
                Some("+ Mặt nạ thuốc bắc gia truyền, xông đầu thảo dược, xông mặt, massage tay"),
            ),
        ],
    },
    SeedCategory {
        name: "Chăm sóc cổ vai gáy",
        services: &[
            s(
                "An Kiên Lạc",
                Some(70),
                Gendered(Some(249_000), Some(299_000)),
                false,
                Some("Đả thông, giảm đau mỏi, đắp thuốc thảo dược"),
            ),
            s(
                "Đại Thông Lạc",
                Some(90),
                Gendered(Some(349_000), Some(399_000)),
                true,
                Some("+ Giác hơi thải độc"),
            ),
        ],
    },
    SeedCategory {
        name: "Chăm sóc body",
        services: &[
            s(
                "Thân Tâm An Định",
                Some(70),
                Gendered(Some(299_000), Some(399_000)),
                false,
                Some("Cổ vai gáy, tay, lưng eo, chân"),
            ),
            s(
                "Trường Xuân Ngọc Cơ",
                Some(120),
                Gendered(Some(399_000), Some(499_000)),
                true,
                Some("+ Xông hơi đá muối, ngâm chân thảo dược"),
            ),
        ],
    },
    SeedCategory {
        name: "Dưỡng sinh chuyên biệt",
        services: &[
            s("Ôn ấm tử cung", Some(120), Unknown, true, None),
            s(
                "Thải độc hệ tiêu hóa – Chăm sóc tỳ vị",
                Some(90),
                Gendered(Some(299_000), Some(349_000)),
                true,
                None,
            ),
            s(
                "Phục hồi sau tai biến đột quỵ",
                Some(140),
                Flat(500_000),
                false,
                None,
            ),
            s("Liệu trình giải cảm", Some(45), Flat(149_000), false, None),
            s(
                "Dưỡng sinh vùng mắt chuyên sâu",
                Some(70),
                Flat(299_000),
                false,
                None,
            ),
            s("Dưỡng sinh thính lực", Some(85), Flat(399_000), false, None),
        ],
    },
    SeedCategory {
        name: "Xông, ngâm và đá muối Himalaya",
        services: &[
            s(
                "Nằm giường đá muối Himalaya + Massage",
                Some(50),
                Flat(299_000),
                true,
                None,
            ),
            s(
                "Tắm ngâm thảo dược + Massage",
                Some(60),
                Flat(299_000),
                false,
                None,
            ),
            s(
                "Tắm ngâm thảo dược + Xông hơi + Massage",
                Some(90),
                Flat(599_000),
                true,
                None,
            ),
        ],
    },
    SeedCategory {
        name: "Chăm sóc chân",
        services: &[
            s(
                "Xông chân thảo dược + Massage chân",
                Some(45),
                Flat(149_000),
                true,
                None,
            ),
            s("Chăm sóc vùng chân", Some(60), Flat(199_000), false, None),
            s(
                "Hỗ trợ phục hồi suy giãn tĩnh mạch",
                Some(60),
                Flat(249_000),
                false,
                None,
            ),
        ],
    },
    SeedCategory {
        name: "Chăm sóc lưng eo",
        services: &[s(
            "Bổ Thận An",
            Some(90),
            Gendered(Some(249_000), Some(299_000)),
            true,
            None,
        )],
    },
    SeedCategory {
        name: "Chăm sóc vùng đầu và giấc ngủ",
        services: &[
            s("Thải độc não bộ", Some(60), Flat(199_000), false, None),
            s(
                "Đau đầu, mất ngủ, giảm stress",
                Some(90),
                Flat(299_000),
                false,
                None,
            ),
        ],
    },
    SeedCategory {
        name: "Chăm sóc da mặt",
        services: &[
            s("Cạo gió mặt", Some(60), Flat(199_000), false, None),
            s("Trẻ hóa làn da", Some(90), Flat(249_000), false, None),
            s(
                "Cấy trắng – Chăm sóc dưỡng sáng da",
                Some(90),
                Flat(299_000),
                false,
                None,
            ),
            s(
                "Cấy tinh chất – Chăm sóc phục hồi da",
                Some(90),
                Flat(349_000),
                false,
                None,
            ),
        ],
    },
    SeedCategory {
        name: "Dịch vụ bổ sung",
        services: &[
            s(
                "Phòng xông hơi đá muối Himalaya",
                Some(20),
                Flat(100_000),
                false,
                None,
            ),
            s(
                "Nằm giường đá muối Himalaya",
                Some(20),
                Flat(100_000),
                false,
                None,
            ),
            s("Mặt nạ thuốc bắc", None, Flat(50_000), false, None),
            s("Tẩy tế bào chết da đầu", None, Flat(20_000), false, None),
            s("Giác hơi + Đắp thuốc bắc", None, Flat(100_000), false, None),
            s("Thải độc đại tràng", None, Flat(149_000), false, None),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_has_the_printed_shape() {
        let services: usize = MENU.iter().map(|c| c.services.len()).sum();
        assert_eq!((MENU.len(), services), (10, 35));
    }
}
