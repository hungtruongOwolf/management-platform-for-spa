---
title: Personas & operating context
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Personas (draft)

## Bối cảnh vận hành (EM xác nhận 2026-11-04)
- 1 cơ sở (Phường Đông Hà); sau này có thể mở nhiều cơ sở.
- 3–8 kỹ thuật viên; **1 quản lý kiêm thu ngân**, cũng là người ghi toàn bộ sổ sách.
- 15–40 lượt khách/ngày.

## 1. Quản lý (kiêm thu ngân): **người dùng chính của MVP**
- Mỗi ngày: đón khách, ghi lượt phục vụ, thu tiền, trừ buổi liệu trình, chấm công, ghi chi phí, chốt quỹ.
- Mỗi tháng (ngày 1–5): chấm đánh giá 6 tiêu chí cho từng nhân viên, làm bảng lương.
- Nỗi đau: một sự việc phải ghi vào nhiều sổ; tính lương, hoa hồng và thưởng bằng tay; không nhớ khách còn mấy buổi.
- Thành công khi: nhập một lượt khách và thanh toán trong **< 30 giây**, cuối ngày chốt quỹ khớp mà không phải cộng tay.

## 2. Chủ spa
- Xem doanh thu, quỹ, công nợ, hiệu suất nhân viên; duyệt bảng lương; quyết định khi có cờ kỷ luật; chỉnh chính sách lương thưởng.
- Thành công khi: nắm tình hình spa trong một màn hình, không cần cầm sổ.

## 3. Kỹ thuật viên
- Xem lịch của mình, số khách đã phục vụ, ngày công, hoa hồng tạm tính và kết quả đánh giá; về sau có thể tự check-in ca.
- Thành công khi: lương minh bạch, tự kiểm tra được từng khoản (đúng cam kết "phiếu lương ghi rõ từng khoản").

## 4. Quản trị nền tảng (EM)
- Tạo spa (tenant), hỗ trợ cấu hình, xử lý sự cố. Không xem dữ liệu khách của spa nếu chưa được spa cho phép (cần chốt chính sách hỗ trợ).

## 5. Khách hàng: giai đoạn sau
- Xem liệu trình còn lại, đặt lịch, nhận nhắc lịch.

## Dữ liệu sức khỏe
Lưu dưới dạng **ghi chú** (khách và từng buổi): chống chỉ định, tình trạng, phản ứng sau buổi. Khách đã đồng ý ngoài đời, platform không quản lý consent. Trách nhiệm pháp lý thuộc EM (quyết định 2026-11-04).

## Thiết bị & kênh (EM xác nhận 2026-11-04)
- Quản lý dùng **mọi loại thiết bị**: desktop, laptop, tablet, điện thoại → **responsive web app**, không cài app.
- **Bắt buộc đăng nhập tài khoản**, không có màn hình dùng chung không cần đăng nhập.
- KTV có smartphone riêng và cần tự xem **lịch, số khách, lương** → giao diện nhân viên ưu tiên mobile.
- Mọi người quen dùng Zalo/Facebook → mức công nghệ bình thường, không cần chế độ đơn giản hóa đặc biệt.
- Giả định **luôn có mạng** (happy case); chưa hỗ trợ offline.
