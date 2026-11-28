---
date: 2026-11-28
source: EM, via the lead (staging = main afb3c97 + e7f7e14)
topic: Vòng quay may mắn — bố cục trang, độ dài cú quay, danh sách, dropdown
---

# EM feedback: vòng quay may mắn, bố cục (2026-11-28)

Nguyên văn:

"thứ nhất tôi muốn thiết kế sao quay sao  cho vòng quay to hơn chiếm phần lớn chủ đạo của trang
tiếp đến là bạn kiêm tra lại trang chính nó đang bị xuất hiện 2 lần người trúng thưởng lược bỏ đi cái ở trên đầu
và thời gian của vòng quay nó nên lâu hơn 3 lần so với hiện tại
rồi nữa đó là bây giờ bạn đang làm là có bao nhiêu data khách hàng là bạn cố để list ra hết nhưng tôi muốn nó nên là 1 component kích thước cứng rồi nếu list dài thì scroll ở trong đó tương tự trong chế độ trình chiếu cách phân phối kích thước của bạn cũng chưa ổn lúc thì cách lề quá nhiều rồi vòng tròn thì điều chỉnh để nó fit vào chứ không phải scroll
rồi hiệu ứng drop down khi bấm vào lựa chọn hình như bây giờ đang dùng library nào đó nên khi nó hiện ra khá là khác biệ so với UI của toàn trang không biết cái này có thiết kế để làm luôn của web app chứ không phải của thư viện được không
nói chung bây giờ cần điều phối sắp xếp lại trang cho rõ ràng"

Lead đã kiểm trên staging (chương trình đã xong "Tri an"): trang là một cột hẹp giữa màn, lề trống lớn trên màn rộng; vòng quay nhỏ (~1/3 cột); chương trình đã xong hiện "Tổng kết chương trình" ở đầu **và** cùng người trúng ở "Danh sách trúng thưởng" (bị lặp). Laptop EM: 1536 × 864 CSS px (Windows 125 %), viewport ~1536 × 730; `prefers-reduced-motion` lúc này là false (EM đang thấy cú quay 4100 ms / 6 vòng). "Dropdown" là `<select>` gốc của ô chọn giải trong trình chiếu (không thư viện nào; popup của hệ điều hành là thứ trông lạ).
