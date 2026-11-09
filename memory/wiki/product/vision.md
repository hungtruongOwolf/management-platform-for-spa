---
title: Product vision & release scope
updated: 2026-11-09
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Vision

**Viên Phúc Y Đường platform** là nền tảng vận hành cho spa dưỡng sinh đông y. Nó thay sổ sách giấy và Excel bằng một nơi nhập liệu duy nhất: quản lý nhập một lần, hệ thống tự ra doanh thu, sổ quỹ, liệu trình còn lại, nhắc nhở và lương thưởng theo đúng quy chế của spa. Dữ liệu được thiết kế đa spa, đa chi nhánh từ đầu; launch cho một spa duy nhất.

Nguyên tắc sản phẩm: **nhập một lần, mọi sổ tự sinh**. Xem [paper-books-analysis](paper-books-analysis.md).

## Kênh truy cập
- **Responsive web app** (desktop, laptop, tablet, điện thoại), bắt buộc đăng nhập. Có thể cài lên màn hình chính như app (PWA).
- **Sẵn sàng cho native app sau này**: mọi chức năng đi qua cùng một Rust API, dùng chung tài khoản và phân quyền. URL ổn định để deep link (web ↔ app, link trong tin nhắn nhắc lịch, link từ trang giới thiệu sang trang đặt lịch).

## Release scope (EM đồng ý 2026-11-04)
| Đợt | Phạm vi |
|---|---|
| **R1: Thay sổ sách** | Đăng nhập + phân quyền cơ bản · bảng giá dịch vụ, gói liệu trình · hồ sơ khách + ghi chú sức khỏe · liệu trình của khách (còn buổi, đã trả, còn nợ, quà tặng) · **màn hình quầy** (lượt khách, KTV, trừ buổi, dịch vụ thêm, thanh toán TM/CK/ghi nợ, người chốt/bán) · sổ quỹ (thu tự động, chi tay, chốt quỹ ngày) · nhắc nhở trong app · báo cáo doanh thu cho chủ spa |
| **R2: Nhân sự & lương** | Hợp đồng nhân viên · chấm công · đánh giá tháng 6 tiêu chí · tự tính lương (lương cứng, trừ ngày nghỉ, hoa hồng làm liệu trình + chốt liệu trình (hoa hồng bán sản phẩm làm khi có module bán hàng), thưởng gắn bó, mức thưởng Tết) · cờ kỷ luật · phiếu lương trên mobile. Điều kiện: giải quyết các ⚠️ trong [compensation-rules](compensation-rules.md); chạy song song Excel 1 tháng |
| **R3: Lịch** | Lịch hẹn nội bộ, KTV xem lịch trên điện thoại |
| **Sau này** | **Bán sản phẩm & tồn kho** (EM 2026-11-04: chưa ưu tiên) · Trang giới thiệu · đặt lịch online theo khu vực · shop online · nhắc qua Zalo/SMS · quản lý nhiều chi nhánh · native app |

> **Thực tế đã build (2026-11-09, chi tiết ở [overview](../overview.md)):** phạm vi trên là kế hoạch gốc. R1 **thu đủ khi ghi lượt, không ghi nợ** (công nợ hoãn, [front-desk-and-packages](specs/front-desk-and-packages.md)); **sổ thu chi / chốt quỹ chưa làm**. Lịch hẹn (R3) đã kéo lên làm trong vòng 4 ([appointments](specs/appointments.md)); KTV xem lịch + công lương của mình trên điện thoại ([staff-management](specs/staff-management.md)). R2 đã có chấm công, đánh giá, bảng lương; hợp đồng, cờ kỷ luật (ngoài cờ nghỉ không phép ≥ 5 ngày) và chi trả theo mốc chưa làm (staff-management §8). Thêm ngoài kế hoạch: console admin nền tảng ([platform-admin](specs/platform-admin.md)).

## Thước đo thành công R1 (sau 1 tháng chạy thật)
1. Quản lý **không còn ghi sổ giấy**: 100% lượt khách và thu chi nằm trên platform.
2. **Chốt quỹ cuối ngày khớp** với tiền mặt và sao kê, không cộng tay.
3. **0 khách bị quên khi hết liệu trình**: mọi liệu trình sắp hết đều được nhắc và có người gọi tư vấn.

## Chuyển đổi dữ liệu
Màn hình **"nhập liệu trình đang dùng dở"**: khách, gói, số buổi còn lại, số tiền còn nợ (khoảng 1–2 phút/khách). Không nhập lại lịch sử từng buổi.

## Thời hạn
Chưa có mốc cụ thể (EM 2026-11-04).

## Hành trình chính R1
1. **Khách đến làm buổi:** tìm khách theo SĐT → dịch vụ lẻ hoặc trừ buổi liệu trình → chọn KTV → dịch vụ dùng thêm → thu TM/CK/ghi nợ.
2. **Bán liệu trình mới:** chọn gói → quà tặng → người tư vấn chốt → trả đủ hoặc trả một phần.
3. **Cuối ngày:** doanh thu ngày → ghi chi → đếm tiền, chốt quỹ → xem nhắc nhở ngày mai.

## Rủi ro (cao → thấp) & cách giảm
1. Nhập trên máy chậm hơn ghi giấy → quản lý bỏ dùng. **Giảm:** prototype màn hình quầy cho quản lý bấm thử trước khi code; mục tiêu < 30 giây/lượt.
2. Tính lương sai → mất niềm tin. **Giảm:** giải quyết 8 ⚠️; dùng ví dụ trong văn bản làm test; chạy song song Excel 1 tháng.
3. Sổ quỹ lệch do sửa/xóa. **Giảm:** append-only, bút toán điều chỉnh, audit, khóa ngày sau khi chốt quỹ.
4. Mất dữ liệu. **Giảm:** backup Supabase + dump hằng đêm ra R2; diễn tập khôi phục hằng tháng.
5. Lộ dữ liệu khách (kể cả qua tài khoản KTV). **Giảm:** phân quyền từ R1; KTV chỉ thấy dữ liệu cần cho buổi làm; mỗi người một tài khoản.
6. Phình phạm vi. **Giảm:** giữ đúng R1; ý tưởng mới ghi vào mục "Sau này".

## Giả định (EM xác nhận 2026-11-04)
- Mỗi buổi chỉ **1 KTV** làm cho 1 khách.
- Giá theo bảng giá cố định; **giảm giá thủ công** trên hóa đơn phải ghi lý do.
- Thanh toán chỉ có **tiền mặt và chuyển khoản**.
