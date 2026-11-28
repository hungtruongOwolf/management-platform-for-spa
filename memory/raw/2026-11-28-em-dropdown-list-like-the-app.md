# EM — danh sách thả xuống của select khác UI cả trang (2026-11-28)

EM xem app và thấy: ô `<select>` đóng đã đúng theme (mũi tên `--select-chevron`, hộp ô chung), nhưng **danh sách mở ra** là của trình duyệt / hệ điều hành, lạc tông với phần còn lại của trang.

Lời EM (nguyên văn, lead chuyển tiếp):

> hiệu ứng drop down khi bấm vào lựa chọn … khi nó hiện ra khá là khác biệt so với UI của toàn trang … có thiết kế để làm luôn của web app

> làm luôn đi nhé

Phạm vi lead giao: **mọi** dropdown của app (26 `<select>` gốc trong 16 file dưới `apps/web/src`).

Quyết định của lead (không bàn lại): dùng customizable select của nền tảng làm progressive enhancement, không viết combobox JS:
- `appearance: base-select` cho `select` và `::picker(select)`, trong `@supports (appearance: base-select)`; hỗ trợ Chrome / Edge 135+, Safari 27; Firefox giữ danh sách gốc.
- Lý do: giữ ngữ nghĩa + bàn phím gốc; `register` của react-hook-form và submit form không đổi; 82 lệnh `selectOptions` (Vitest) và 44 `selectOption` (Playwright) chạy y nguyên; danh sách nằm ở top layer nên dùng được trong sheet `<dialog>` và trình chiếu vòng quay.
- Không thay `<select>` bằng component tự viết, không đổi markup trừ khi một cải tiến thật sự cần.
- Danh sách phải giống menu của app (`RowMenu`, `CommandPalette`): nền, viền, bo góc, bóng, đệm; dòng cao, hover / focus / đang chọn, option bị tắt (mờ nhưng đọc được), nhãn `<optgroup>`, cao tối đa có cuộn trong, rộng ít nhất bằng ô; sáng + tối; chuyển động mở / đóng theo token, không chuyển động khi reduced motion; điện thoại ≤ 768px vùng chạm ≥ 44px, vừa màn; WCAG 2.2 AA; ô đóng phải trông y như hiện tại.

Nguồn lead chỉ định: developer.chrome.com/blog/a-customizable-select, MDN "Customizable select", webkit.org/blog/18117 (golden rule), caniuse customizable-select.
