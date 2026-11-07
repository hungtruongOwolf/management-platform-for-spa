---
source: EM decisions 2026-11-07 + live-test findings, relayed by the lead agent to the appointments agent
date: 2026-11-07
---

# Lịch hẹn: quyết định EM 2026-11-07 + phát hiện khi test thật (chép từ brief của lead)

1. **"Hủy lịch" phải là nút thật.** EM: "hủy lịch cái đó có thể làm thành button đặt lại vị trí cho hợp lý … bây giờ nó đang rất tệ là chỉ một cái text … cũng phải là 1 button check kĩ lại đi".
   - Trong sheet lịch hẹn, hủy là một link chữ. Làm thành nút (kiểu destructive/secondary của design system, cùng cỡ control với các nút bên cạnh), đặt hợp lý trong vùng thao tác.
   - Kiểm các thao tác liên quan ("Không đến", "Đổi lịch", "Khách đến · ghi lượt"), cả ở 390px.
2. **Quyết định A: thời lượng chính xác.** Dịch vụ 70 phút đang chiếm 75 phút (09:00–10:15, làm tròn lên ô 15 phút). Khối lịch, giờ kết thúc, kiểm trùng và sức chứa dùng đúng thời lượng: 09:00–10:10. Server làm tròn thì sửa cả server, có test. Lịch phải vẽ đúng chiều cao không khớp ô.
3. **Một khách đặt trùng giờ.** Cùng một khách đặt hai lịch chồng giờ mà không có cảnh báo (Nguyễn Thị Lan Anh 09:00 ×2). Thêm cảnh báo hoặc server từ chối với câu rõ ràng; chọn và giải thích. Người đi cùng trong một nhóm không tính là trùng.
4. **Lịch trong quá khứ.** Đặt được lịch ở quá khứ mà không cảnh báo. Cảnh báo hoặc chặn; lượt nhập bù là luồng riêng ở Quầy.
5. **Lỗi form đặt lịch ngoài màn hình.** Trùng KTV, "Còn thiếu: khách, dịch vụ…" và cảnh báo sức chứa hiện ở cuối form dài, ngoài khung nhìn. Làm cho thấy được: cuộn + focus lỗi đầu tiên, và/hoặc tóm tắt cạnh nút gửi.
6. **KTV mặc định.** "KTV làm" mặc định là KTV đầu tiên (Lê Thị Hoa) khi check-in lịch chưa gán. Phải mặc định trống, hoặc KTV đã gán của lịch, và bắt buộc chọn.
- **Quyết định B không đổi:** "Đang làm" giữ tới hết khung giờ.
