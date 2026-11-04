---
title: Phân tích sổ sách giấy hiện tại → mô hình số hóa
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Phân tích 4 cuốn sổ giấy

## Hiện trạng
| Sổ | Ghi gì | Cột |
|---|---|---|
| **Sổ dịch vụ** (sổ chung) | Mỗi lượt phục vụ | Ngày, Khách, Dịch vụ, Giá, Nhân viên, Ghi chú |
| **Sổ doanh thu hằng ngày** | Tiền bán dịch vụ/liệu trình | Ngày ghi sổ, Dịch vụ 1 lần / Liệu trình, Chuyển khoản, Tiền mặt, **Chưa thanh toán**, Ghi chú |
| **Sổ theo dõi liệu trình khách hàng** | 1 trang / khách / liệu trình | Header: Khách, SĐT, Dịch vụ, Giá, Ngày đăng ký, **Quà tặng**. Dòng: STT, Ngày/**Giờ hẹn**, DV sử dụng, **DV dùng thêm**, Giá, **Giờ ra**, Ghi chú |
| **Sổ thu chi hằng ngày** | Sổ quỹ | Ngày, Chứng từ Thu/Chi, Diễn giải, Thu (TM/CK), Chi (TM/CK), **Tồn** (TM/TK) |
| *(Excel)* Bảng chấm công | Mỗi NV mỗi tháng | Đi làm, giờ đến/về, đi muộn, nghỉ P/ĐX/K, **số khách**, khen/phàn nàn |

## Vấn đề cốt lõi: một sự kiện bị ghi tay 3–5 lần
Khi một khách đến làm 1 buổi liệu trình rồi mua thêm dịch vụ, người ghi phải viết vào:
1. sổ dịch vụ,
2. trang liệu trình của khách (trừ buổi, giờ ra),
3. sổ doanh thu (nếu trả tiền),
4. sổ thu chi (tiền vào quỹ),
5. bảng chấm công (cộng "số khách" cho NV), và về sau còn phải tính hoa hồng bằng tay.

Hậu quả: sai lệch giữa các sổ, không tính được hoa hồng (chính file Excel đã ghi nhận điều này), không biết khách còn bao nhiêu buổi, không có nhắc nhở, công nợ ("chưa thanh toán") dễ thất lạc.

## Mô hình mới: **nhập một lần, mọi sổ tự sinh**
Sự kiện gốc chỉ có 3 loại, mọi báo cáo khác đều được *tính ra*:
- **Lượt phục vụ (Visit)**: khách nào, ngày giờ vào/ra, KTV nào làm dịch vụ nào, buổi đó lấy từ liệu trình nào hay là dịch vụ lẻ, dịch vụ dùng thêm.
- **Hóa đơn + Thanh toán (Sale/Payment)**: bán dịch vụ lẻ, liệu trình, sản phẩm. Thanh toán tiền mặt hoặc chuyển khoản, cho phép trả nhiều lần; phần còn thiếu là công nợ. Người tư vấn/chốt được gắn vào hóa đơn để tính hoa hồng.
- **Phiếu chi / phiếu thu khác (Cash entry)**: chi phí vận hành, rút/nộp quỹ.

| Sổ giấy cũ | Trên platform |
|---|---|
| Sổ dịch vụ | Danh sách lượt phục vụ (lọc theo ngày, NV, dịch vụ) |
| Sổ doanh thu | Báo cáo doanh thu tự tính từ hóa đơn: TM/CK, dịch vụ lẻ vs liệu trình (công nợ: hoãn, R1 thu đủ) |
| Sổ liệu trình | Hồ sơ khách → liệu trình đã mua: tổng/đã dùng/còn lại, quà tặng, lịch sử buổi |
| Sổ thu chi | Sổ quỹ: thu tự động từ thanh toán, chi nhập tay; tồn quỹ TM / tài khoản tính tự động; chốt quỹ cuối ngày |
| Chấm công: số khách | Tự đếm từ lượt phục vụ |

## Nhắc nhở tự động (thứ sổ giấy không làm được)
- Liệu trình **sắp hết** (còn ≤ N buổi, mặc định 2) → báo quản lý để tư vấn gia hạn.
- Liệu trình **đã hết** hoặc **sắp hết hạn** sử dụng (nếu có hạn).
- Khách **còn buổi nhưng lâu không quay lại** (≥ X ngày, ví dụ 7 hoặc 14) → báo quản lý để chủ động follow up. **N và X do chủ spa tùy chỉnh** (EM 2026-11-04).
- Lịch hẹn ngày mai (cho lễ tân/NV).
- Chênh lệch quỹ khi chốt cuối ngày.
Kênh nhắc: trong app trước (thông báo cho quản lý/lễ tân); Zalo/SMS gửi khách để sau.
