---
title: Kịch bản buổi dùng thử R1-1 với quản lý
updated: 2026-11-04
sources: [product/specs/front-desk-and-packages.md, plans/r1-1-front-desk.md, design/front-desk-flows.md]
---

# Kịch bản buổi dùng thử R1-1 (màn hình quầy & liệu trình)

Mục đích: quản lý (kiêm thu ngân) tự làm các việc hằng ngày trên platform bằng **dữ liệu giả**, để kiểm tra spec R1-1 đã thay được sổ dịch vụ, sổ liệu trình và phần bán hàng của sổ doanh thu chưa. Ticket S10 chỉ được tick sau buổi này. Thời lượng khoảng **60–75 phút**: 10 phút chuẩn bị, 40 phút làm việc, 15 phút góp ý.

Vai trò:
- **Người dẫn** (EM hoặc dev): đọc nhiệm vụ, bấm giờ, ghi chép. Không chỉ tay lên màn hình, không gợi ý nút nào, trừ khi quản lý bí quá 2 phút (ghi lại là "cần gợi ý").
- **Quản lý**: làm như ngày thường, vừa làm vừa nói to điều mình đang nghĩ ("tôi đang tìm chỗ…").
- **Chủ spa** (tùy chọn): làm nhiệm vụ 12–13 (hủy ngày cũ, hoàn tiền) nếu có mặt; nếu không, người dẫn đăng nhập bằng tài khoản chủ spa.

## 1. Chuẩn bị trên máy dev (người dẫn, làm trước buổi 1 ngày)

Yêu cầu: Rust stable, Node 22, pnpm 10, Docker Desktop. Trình duyệt Chrome hoặc Edge bản mới. Nên dùng laptop/tablet có bàn phím (thiết bị quầy thật nếu có).

```bash
cp .env.example .env               # điền APP_DATA_KEY: openssl rand -base64 32
pnpm install
pnpm dev:db
docker exec vpyd-dev-postgres-1 psql -U vpyd -d vpyd -c "CREATE DATABASE vpyd_trial"
# Trỏ DATABASE_URL và APP_DATABASE_URL trong .env sang /vpyd_trial (dữ liệu dùng thử tách riêng)
SQLX_OFFLINE=true pnpm db:migrate
A="cargo run --manifest-path apps/api/Cargo.toml -p app --"
$A create-tenant --slug dung-thu --name "Viên Phúc Y Đường (dùng thử)" --branch "Đông Hà"
APP_NEW_USER_PASSWORD='<mật-khẩu-chủ>'   $A add-user --tenant dung-thu --phone 0905000001 --name "Chủ spa" --role owner
APP_NEW_USER_PASSWORD='<mật-khẩu-quản-lý>' $A add-user --tenant dung-thu --phone 0905000002 --name "<tên quản lý>" --role manager
$A seed-catalog --tenant dung-thu
$A add-staff --tenant dung-thu --name "Lan"  --position "Kỹ thuật viên"
$A add-staff --tenant dung-thu --name "Tuấn" --position "Kỹ thuật viên"
$A add-staff --tenant dung-thu --name "Hoa"  --position "Kỹ thuật viên"
pnpm dev:api     # cửa sổ 1
pnpm dev:web     # cửa sổ 2 → http://localhost:5173
```

Sau đó, đăng nhập bằng **chủ spa** (lần đầu phải quét mã QR bằng Google/Microsoft Authenticator) và tạo sẵn:
1. Gói **"Gội đầu dưỡng sinh 10 buổi + tặng 2"**, dịch vụ "Cơ Bản – Dầu thảo dược" × 10 + tặng 2, giá 1.500.000đ, hạn 6 tháng (trang Bảng giá → Gói liệu trình).
2. Gói **"Cổ vai gáy 5 buổi"**: dịch vụ "An Kiên Lạc" × 5, giá 1.000.000đ, không hạn.
3. 3 khách quen ở `/khach-hang`: **Nguyễn Thị Hà** (Nữ, 0905 111 222, ghi chú sức khỏe "Huyết áp cao, không bấm huyệt mạnh"), **Trần Văn Nam** (Nam, 0905 333 444), **Lê Thị Mai** (chưa ghi giới tính, 0905 555 666).
4. Bán cho chị Hà gói Gội đầu 10 + 2 (TM 1.500.000đ), rồi ghi 1 lượt trừ 1 buổi để gói còn **11/12**.

Kiểm tra trước buổi: đăng nhập được bằng tài khoản **quản lý**, `/quay` hiện ô tìm khách, danh sách lượt hôm nay có 1 lượt. Chuẩn bị đồng hồ bấm giờ (điện thoại) và in phiếu góp ý ở §4.

## 2. Nhiệm vụ (quản lý làm, người dẫn bấm giờ)

Đọc nhiệm vụ cho quản lý **bằng lời của khách**, không đọc tên nút. Bấm giờ từ lúc đọc xong đến lúc màn hình báo "đã lưu". Mục tiêu ghi một lượt **< 30 giây**.

| # | Tình huống (người dẫn đọc) | Thao tác mong đợi | Bấm giờ? | Mục tiêu |
|---|---|---|---|---|
| 1 | "Đăng nhập vào máy quầy như đầu ca." | `/dang-nhap` bằng SĐT + mật khẩu quản lý | — | — |
| 2 | "Chị Hà vào gội đầu theo liệu trình, KTV Lan làm." | `/quay` → gõ SĐT/tên → Trừ 1 buổi → chọn Lan → Lưu | ✓ | < 30 s, ≤ 4 thao tác; toast "còn 10/12" |
| 3 | Làm lại nhiệm vụ 2 chỉ bằng **bàn phím** (gõ tên "ha" không dấu, Enter, Enter, chọn KTV, Ctrl+Enter) | Luồng nhanh bàn phím | ✓ | < 20 s |
| 4 | "Anh Nam (khách nam) làm Đại Thông Lạc lẻ, trả tiền mặt, KTV Tuấn." | Dịch vụ lẻ → giá Nam **399.000đ** tự chọn → TM → Lưu | ✓ | < 30 s; giá đúng giới tính |
| 5 | "Chị Mai chưa có giới tính trong hồ sơ, làm Thư Giãn lẻ, chuyển khoản." | Hệ thống bắt chọn Nữ/Nam cho dòng → chọn Nữ (79.000đ) → CK → Lưu | ✓ | < 40 s |
| 6 | "Chị Hà hôm nay gội đầu theo gói và làm thêm xông chân + massage chân lẻ (149.000đ), trả 50.000đ TM + 99.000đ CK." | 1 dòng trừ buổi + 1 dòng lẻ, thanh toán tách 2 hình thức | ✓ | < 45 s |
| 7 | "Khách mới tên **Phạm Thị Thu**, SĐT 0912 345 678, vào gội đầu lẻ, KTV Hoa, tiền mặt." | Tạo khách ngay trên quầy → dòng lẻ → Lưu | ✓ | < 60 s |
| 8 | "Anh Nam mua gói Cổ vai gáy 5 buổi, chị (quản lý) chốt, giảm 50.000đ vì khách quen, trả CK." | Hồ sơ/thẻ khách → Bán liệu trình → người chốt → giảm giá + lý do → CK | ✓ | < 60 s; giá trị buổi = 190.000đ |
| 9 | "Thử giảm 20% cho chị Mai khi bán gói Gội đầu." | Hệ thống từ chối (quản lý tối đa 10%) với thông báo dễ hiểu | — | Quản lý hiểu vì sao bị chặn |
| 10 | "Lúc nãy ghi nhầm lượt của chị Mai (nhiệm vụ 5), hủy giúp." | Danh sách lượt hôm nay → Hủy → nhập lý do | ✓ | < 30 s; lượt hiện "Đã hủy" + lý do |
| 11 | "Đây là trang sổ liệu trình giấy của **bà Võ Thị Lài** (SĐT 0935 222 111): gói Gội đầu 10 buổi, đã dùng 6, giá 1.500.000đ, đăng ký 01/06/2026, không hạn." | `/nhap-so-cu` → khách mới → gói từ bảng giá → còn 4/10 → Lưu | ✓ | 1–2 phút; hồ sơ hiện "Nhập từ sổ cũ" |
| 12 | (Chủ spa) "Hủy lượt của **hôm qua**" — người dẫn chuẩn bị 1 lượt ngày cũ trước buổi, hoặc bỏ qua nếu không có | Xem ngày → Hủy → hệ thống hỏi lại mật khẩu (+ mã 2 bước) | — | Quản lý thử trước: thấy "Ngày đã chốt" |
| 13 | (Chủ spa) "Chị Hà chuyển nhà, hủy gói và hoàn 300.000đ tiền mặt." | Hồ sơ → Hủy liệu trình + hoàn tiền → xác nhận lại mật khẩu | — | Gói "Đã hủy", còn 0 buổi |
| 14 | "Khách hỏi: chị Hà còn mấy buổi, hết hạn khi nào? Lần trước ai làm?" | Tìm khách → hồ sơ: còn X/Y, HSD, lịch sử buổi | ✓ | < 15 s |
| 15 | "Cuối ca, xem hôm nay KTV Lan làm cho mấy khách." | Danh sách lượt hôm nay (đếm theo KTV) | — | Quản lý tìm được hay không |

Ghi chú cho người dẫn:
- Nhiệm vụ 2, 3 là **thước đo chính** (mục tiêu < 30 s/lượt, spec §1). Nếu > 30 s, hỏi ngay "chỗ nào làm chị chậm?".
- Nhiệm vụ 9, 12 kiểm tra **thông báo lỗi**: quản lý có hiểu lý do bị chặn không, có biết phải làm gì tiếp không.
- Nhiệm vụ 14: hồ sơ hiện còn X/Y buổi và HSD, nhưng **lịch sử buổi làm trên hồ sơ chưa có** (`GET /customers/{id}/visits` hoãn từ S7; hiện chỉ xem được theo ngày ở `/quay`). Ghi lại quản lý tìm "lần trước ai làm" ở đâu và có cần không: đây là đầu vào để quyết định làm trước S11 hay không.
- Nhiệm vụ 15 có thể chưa có màn hình riêng (báo cáo thuộc spec #3); ghi lại quản lý muốn xem ở đâu.
- Nếu mạng/app lỗi giữa chừng: ghi lại, bấm Lưu lại; dữ liệu đã nhập phải còn nguyên và không bị ghi 2 lần.

## 3. Bảng bấm giờ (người dẫn điền)

| # | Thời gian (giây) | Số lần bấm/gõ phím | Cần gợi ý? | Lỗi/nhầm | Ghi chú lời quản lý nói |
|---|---|---|---|---|---|
| 2 | | | | | |
| 3 | | | | | |
| 4 | | | | | |
| 5 | | | | | |
| 6 | | | | | |
| 7 | | | | | |
| 8 | | | | | |
| 10 | | | | | |
| 11 | | | | | |
| 14 | | | | | |

**Đạt** khi: nhiệm vụ 2–5 đều < 30 s (5 có thể < 40 s), không nhiệm vụ nào cần gợi ý quá 1 lần, không có lỗi làm sai dữ liệu (sai số buổi, sai tiền).

## 4. Phiếu góp ý (quản lý điền sau buổi)

Thang 1–5 (1 = rất không đồng ý, 5 = rất đồng ý):

| Câu | 1 | 2 | 3 | 4 | 5 |
|---|---|---|---|---|---|
| Ghi một lượt khách trên máy nhanh hơn ghi sổ giấy | | | | | |
| Tôi tìm được khách nhanh, kể cả khi gõ không dấu | | | | | |
| Tôi thấy rõ lưu ý sức khỏe của khách trước khi làm | | | | | |
| Tôi tin số buổi còn lại trên máy là đúng | | | | | |
| Giá dịch vụ lẻ (Nữ/Nam) hiện đúng, tôi không phải tự nhớ | | | | | |
| Bán gói và ghi giảm giá dễ hiểu | | | | | |
| Hủy lượt ghi nhầm dễ, tôi không sợ làm hỏng số liệu | | | | | |
| Nhập liệu trình từ sổ cũ đủ nhanh để nhập hết các khách đang dùng gói | | | | | |
| Chữ, nút, màu dễ đọc trên máy quầy | | | | | |
| Tôi sẵn sàng bỏ sổ dịch vụ và sổ liệu trình giấy nếu dùng hệ thống này | | | | | |

Câu hỏi mở:
1. Việc nào hôm nay chị thấy **chậm hoặc khó** nhất? Vì sao?
2. Có việc nào ở quầy mà sổ giấy làm được nhưng máy **chưa làm được** không?
3. Thông báo nào chị **không hiểu**?
4. Nếu dùng từ tuần sau, chị lo nhất điều gì (mất mạng, mất điện, nhầm khách, …)?
5. Chị muốn thêm/bớt gì trên màn hình quầy?
6. Mỗi ngày có khoảng bao nhiêu khách đang dùng gói cần nhập từ sổ cũ? (để ước lượng thời gian chuyển đổi)

## 5. Sau buổi (người dẫn)

- Ghi kết quả vào `memory/raw/2026-11-xx-trial-session-notes.md` (bảng bấm giờ + phiếu góp ý, nguyên văn), rồi chạy `memory` ingest.
- Mỗi vấn đề → một dòng trong plan (lỗi → ticket fix; thiếu tính năng → backlog spec #2/#3/#4).
- Đạt tiêu chí §3 và câu cuối của phiếu ≥ 4 → tick S10 trong [r1-1-front-desk](r1-1-front-desk.md) và chuyển sang S11 (staging).
- Xóa DB `vpyd_trial` sau khi ghi nhận xong (dữ liệu giả, nhưng có tên/SĐT đọc to trong buổi).
