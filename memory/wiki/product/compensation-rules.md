---
title: Quy tắc chấm công, đánh giá, lương thưởng (trích từ tài liệu Spa)
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Quy tắc lương thưởng hiện hành

Nguồn: 2 văn bản chương trình (học viên, KTV có kinh nghiệm) + file Excel chấm công. Trong hệ thống, mọi con số dưới đây là **cấu hình theo phiên bản (policy)** chứ không được hard-code — xem [architecture/principles](../architecture/principles.md).

## 1. Hai chương trình hợp đồng
| | Học viên (chưa kinh nghiệm) | KTV có kinh nghiệm |
|---|---|---|
| Giai đoạn đầu | Đào tạo 4 tháng | Thử việc 1 tháng (bắt buộc có chứng chỉ hành nghề) |
| Đặt cọc | Chọn: có cọc 5.000.000đ / không cọc | Không có khái niệm cọc |
| Gói cam kết | 12 hoặc 24 tháng sau đào tạo | 12 hoặc 24 tháng sau thử việc |
| Thưởng gắn bó 500k/tháng | Chỉ người **không cọc** | **Mọi người** trong HĐ cam kết |
| Gói 24 tháng | Spa hỗ trợ chứng chỉ 4tr: hoàn 2tr ở mốc 12, 2tr ở mốc 24 | "Thưởng cam kết" 4tr (xem ⚠️ #1) |
| Phí đồng phục | 700.000đ (không hoàn) | — |

**Học viên:** T1 hỗ trợ 1tr (cả hai); T2 hỗ trợ 2–3tr (cọc) / 1–2tr (không cọc) theo năng lực; T3–4 lương theo mức đánh giá: Mức 1 = 4tr/3tr, Mức 2 = 3,5tr/2,5tr, Mức 3 hoặc Chưa đạt = 3tr/2tr (cọc/không cọc).
**Thử việc KTV:** < 1 năm KN 4tr; 1–<3 năm 4,5tr; ≥ 3 năm 5tr; không có hoa hồng.

## 2. Lương cứng nhân viên chính thức
- Lựa chọn lương (cố định suốt HĐ): **LC1** = mức chuẩn, HH 5%; **LC2** = chuẩn − 500k, HH 7%; **LC3** = chuẩn − 1tr, HH 10%.
- Mức chuẩn năm 1 theo kinh nghiệm: < 1 năm 5tr; 1–<3 năm 5,5tr; 3–<5 năm 6tr; ≥ 5 năm 6,5tr. Học viên sau đào tạo tính như < 1 năm.
- **Tăng 10%/năm làm việc**, tính trên mức năm trước, làm tròn đến nghìn đồng (bảng 10 năm có sẵn). Khoản trừ của LC2/LC3 là số cố định, không tăng theo năm.
- Trong HĐ cam kết, ai được Thưởng gắn bó thì lương cứng thực nhận **thấp hơn 500k**. Hết HĐ thì trở lại mức bảng, không còn Thưởng gắn bó.

## 3. Hoa hồng (theo % của lựa chọn lương), 3 loại
1. Làm liệu trình: % × giá trị liệu trình **trực tiếp làm**.
2. Chốt liệu trình mới: % × giá trị liệu trình mới **tư vấn và chốt**.
3. Bán sản phẩm: % × giá trị sản phẩm bán.

**EM 2026-11-06 (Q-U2, Q-U3, [raw](../../raw/2026-11-06-em-answers-round6-upsell.md)):** loại 2 hiểu là **hoa hồng giới thiệu / chốt (upsell)**, tính trên **cả gói liệu trình lẫn dịch vụ lẻ** do người đó giới thiệu được; loại 1 (**làm**) vẫn cho người trực tiếp làm. Hai khoản tách biệt, có thể là hai người. Mỗi thứ bán ra có **đúng một** người giới thiệu: chủ spa, quản lý, một KTV, hoặc "khách tự chọn" (không ai được tính). App: `sale.closer_staff_id` (gói), `visit_line.referrer_staff_id` (dịch vụ lẻ), xem [staff-management](specs/staff-management.md) AC5.2.

## 4. Ngày nghỉ & chấm công
- Ký hiệu: ✓ đi làm; **P** có lý do, báo trước ≥ 3 ngày; **ĐX** đột xuất (bổ sung giấy tờ sau); **K** không lý do / không báo / không bổ sung giấy.
- Mỗi tháng được nghỉ **2 ngày không trừ lương**. Mỗi ngày vượt quá bị trừ 1 ngày lương.
- **Lương ngày = lương cứng tháng ÷ (số ngày trong tháng − 2).**
- Đi muộn = giờ đến − giờ vào ca (phút), chỉ ghi nhận, chưa có quy tắc phạt (⚠️ #4).

## 5. Đánh giá năng lực tháng (100 điểm), chấm ngày 1–5 tháng sau
Tiêu chí (điểm tối đa): Kỹ thuật liệu trình 40 · Thái độ & giao tiếp 15 · Phản hồi khách 15 · Chuyên cần 10 · Vệ sinh & tác phong 10 · Tinh thần học hỏi 10.
- Mức cho từng tiêu chí: Mức 1 = 100%, Mức 2 = 80%, Mức 3 = 60%, Chưa đạt ≤ 30% (có thể chấm thấp hơn).
- Xếp loại tháng: ≥ 85 **Mức 1**; 70–84 **Mức 2**; 50–69 **Mức 3**; < 50 **Chưa đạt**.
- Có khiếu nại nghiêm trọng hoặc vi phạm nội quy nhiều lần trong tháng → kết quả tối đa **Mức 3**.
- Mức 3 / Chưa đạt → **bắt buộc** ghi kế hoạch kèm cặp tháng sau.
- Cuối thử việc đạt ≥ Mức 2 → đủ điều kiện ký HĐ chính thức.

## 6. Thưởng Tết = tổng tiền của từng tháng trong năm
Mức tháng = **mức thấp hơn** giữa mức theo ngày nghỉ và mức theo đánh giá:
| Mức | Ngày nghỉ | Đánh giá | Tiền |
|---|---|---|---|
| 1 | < 2 ngày | Mức 1 | 500.000đ |
| 2 | đúng 2 ngày | Mức 2 | 400.000đ |
| 3 | 3–4 ngày | Mức 3 | 300.000đ |
| Không tính | > 4 ngày | Chưa đạt | 0đ |

## 7. Thưởng gắn bó & các mốc chi trả
- Cộng 500k mỗi tháng làm việc trong HĐ, kể cả tháng có ngày nghỉ. Khoản này được ghi trên phiếu lương.
- Gói 12 tháng: nhận đủ 6tr ở mốc 12. Gói 24 tháng: mốc 12 nhận 3tr (50% năm 1), mốc 24 nhận 9tr.
- Trả cùng kỳ lương (ngày 1–5 tháng sau mốc).
- Nghỉ ngang không lý do chính đáng → mất cọc hoặc mất phần Thưởng gắn bó chưa tới mốc.
- Spa chấm dứt HĐ do lỗi NV (Nhóm A) → giữ lương và hoa hồng đến ngày cuối, mất phần chưa tới mốc. Không do lỗi NV (Nhóm B) → nhận ngay toàn bộ + hoàn cọc / chứng chỉ.

## 8. Phúc lợi
Tiền ăn 300–600k/tháng · BHXH 308k/tháng từ tháng thứ 13 HĐ chính thức · BHYT (chưa có số) · quà lễ.

## 9. Cảnh báo kỷ luật (hệ thống **chỉ gắn cờ**, chủ spa quyết định)
- Chưa đạt **2 tháng liên tiếp**, hoặc **3 trong 6 tháng** gần nhất (Nhóm A #5).
- Nghỉ **K ≥ 5 ngày** cộng dồn trong tháng (Nhóm A #8, tự ý bỏ việc).
- Nhắc nhở bằng văn bản **≥ 3 lần trong 6 tháng** (Nhóm A #7).
- Khiếu nại nghiêm trọng (Nhóm A #6).

## ⚠️ Mâu thuẫn / chỗ còn thiếu cần EM xác nhận
Mặc định đã build và quy tắc chỉnh được cho từng mục: [spec staff-management §6](specs/staff-management.md) (2026-11-05). Vẫn chờ EM trả lời (Q-C1…Q-C8).
1. **Thưởng cam kết 24 tháng (KTV có KN):** mục 1 ghi "trả 2tr ở mốc 12 + 2tr ở mốc 24", nhưng mục 3.5 và 4 ghi "chỉ trả khi đủ 24 tháng".
2. **2 ngày nghỉ miễn trừ** áp dụng cho cả P, ĐX và K, hay K luôn bị trừ? Excel đang đếm chung cả ba.
3. **Hoa hồng làm liệu trình**: một buổi lấy từ liệu trình trả trước được tính giá trị bao nhiêu (giá gói ÷ số buổi)? Chốt liệu trình mà khách trả góp/còn nợ thì tính hoa hồng trên tiền đã thu hay trên giá trị bán? Hai KTV cùng làm một khách thì chia thế nào?
4. **Đi muộn**: chỉ ghi nhận hay có phạt? Ngưỡng bao nhiêu phút?
5. **Hỗ trợ tiền ăn 300–600k** và **hỗ trợ học viên tháng 2 (khoảng)**: dựa vào đâu để chọn con số? Quản lý nhập tay hay có quy tắc?
6. **Ngày lễ / Chủ nhật**: có tính ngày công đặc biệt không?
7. "Tháng làm việc" để cộng Thưởng gắn bó: vào làm giữa tháng thì tính thế nào?
8. Excel công thức lương "trong HĐ cam kết" luôn trừ 500k, kể cả học viên **có cọc**, trong khi văn bản nói người có cọc không bị trừ. Đây có thể là lỗi do Excel chỉ mô phỏng chương trình KTV.
