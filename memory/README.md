# memory/ — Bộ nhớ dự án (Karpathy LLM Wiki)

Thay vì để agent "quên" sau mỗi phiên chat, mọi kiến thức của dự án được **biên soạn thành wiki markdown** và agent tự duy trì. Mô hình theo ý tưởng *LLM Wiki* của Andrej Karpathy, gồm 3 lớp:

| Lớp | Vị trí | Ai viết | Quy tắc |
|---|---|---|---|
| **Raw sources** | `raw/` | EM + agent (copy nguồn vào) | Bất biến — chỉ thêm, không sửa. Đây là "sự thật gốc". |
| **Wiki** | `wiki/` | Agent | Kiến thức đã tổng hợp, liên kết chéo, luôn cập nhật. |
| **Schema** | `../CLAUDE.md` + skill `memory` | EM + agent | Luật vận hành: cấu trúc, quy ước, quy trình. |

Hai file điều hướng:
- `index.md` — mục lục toàn bộ wiki (mỗi trang 1 dòng). Agent đọc file này đầu tiên.
- `log.md` — nhật ký theo thời gian, chỉ append.

## 4 thao tác (chi tiết trong `.claude/skills/memory/SKILL.md`)
- **ingest** — đưa nguồn mới vào `raw/`, cập nhật các trang wiki liên quan, index, log.
- **query** — trả lời câu hỏi từ wiki; câu trả lời có giá trị thì lưu lại thành trang.
- **lint** — rà soát mâu thuẫn, trang lỗi thời, trang mồ côi, link hỏng.
- **record** — cuối mỗi phiên làm việc có ý nghĩa: ghi quyết định, tiến độ.

## Cấu trúc wiki
```
wiki/
├── overview.md        # Bức tranh tổng: dự án là gì, đang ở giai đoạn nào
├── product/           # Vision, personas, glossary, specs/ (PRD từng tính năng)
├── design/            # Design system, UX flows, prototype notes
├── architecture/      # system.md, backend.md, frontend.md, data.md
├── decisions/         # ADR-XXXX — các quyết định kỹ thuật quan trọng
├── security/          # Threat model, authz matrix, content-protection plan
├── plans/             # Breakdown thành ticket/slice, checklist tiến độ
├── research/          # Nghiên cứu công nghệ, so sánh lựa chọn
└── conventions/       # Coding standards, git workflow
```

Mẹo: mở thư mục `memory/` bằng **Obsidian** để xem graph liên kết giữa các trang.
