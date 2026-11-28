---
date: 2026-11-28
source: EM, via the lead (staging = main 6554630, PR #211 live)
topic: Vòng quay may mắn — cú quay bị giật
---

# EM: vòng quay bị giật khi quay (2026-11-28)

Nguyên văn:

"bạn có để ý lúc bấm quay nó cứ bị giật giật không không biết cái này ó phải là do setting của chrome hay không bạn check lại tôi với"

Lead đã đo trên máy EM: Chrome vẽ bằng GPU rời NVIDIA RTX 3050 Ti (D3D11, tăng tốc phần cứng bật); máy cắm sạc 100 % (không ở chế độ tiết kiệm pin); `prefers-reduced-motion` false; màn 1920 × 1080 @ 120 Hz trên GPU tích hợp Intel (laptop hai GPU), Chrome vẽ trên GPU rời; mặt quay rộng 516 px ở viewport 1920 × 912. Không đo được nhịp khung hình trong Chrome của EM (tab bị che, rAF chạy 1 Hz).

Giả thuyết của lead: không phải do cài đặt Chrome. Với `cubic-bezier(0.13, 0.66, 0.04, 1)` và 3262,5° trong 6 s, tốc độ lúc đầu tới ~55°/khung ở 60 Hz (~28° ở 120 Hz), tức ~250 px/khung ở mép mặt quay 516 px, và trên 15°/khung suốt ~1 s đầu; vòng 8 cánh (45°/cánh) ở tốc độ đó nhấp nháy, giật (45°/khung thì trông như đứng yên). Bản HTML gốc cùng tốc độ góc nhưng vòng nhỏ hơn nhiều; bố cục mới làm bước nhảy mỗi khung lộ rõ.
