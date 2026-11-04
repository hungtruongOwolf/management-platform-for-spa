---
title: Code intelligence tools for agents (impact analysis)
updated: 2026-11-04
sources: [raw/2026-11-04-idea-internal-platform.md]
---

# Công cụ giúp agent nắm quan hệ trong code

Vấn đề EM nêu: agent chỉ sửa đúng chỗ đang nhìn, bỏ sót nơi gọi tới và test cũ. Cách giải chia 3 tầng, quy trình chi tiết nằm ở `.claude/skills/build/impact-analysis.md`.

| Tầng | Công cụ | Vai trò | Quyết định |
|---|---|---|---|
| Ngôn ngữ / compiler | Rust type system, TS `strict`, OpenAPI → client TS sinh tự động | Đổi chữ ký là compile fail ngay, kể cả ở frontend | **Có từ ngày đầu** |
| LSP cho agent | Plugin chính thức `rust-analyzer-lsp` và `typescript-lsp` (marketplace claude-plugins-official), cần binary `rust-analyzer` (✅ đã cài qua rustup, S0) và `typescript-language-server` | Find references / go to definition chính xác thay vì grep | **EM chạy `/plugin install`** (S0) |
| Semantic MCP | Serena (điều khiển LSP thật, thao tác theo symbol) | Thay thế/bổ sung cho LSP plugin | Cân nhắc nếu LSP plugin chưa đủ |
| Code graph MCP | codebase-memory-mcp, CodeGraph và các tool tương tự (impact / blast radius) | Đồ thị call/dependency | **Chưa dùng**: còn non, chủ yếu tự quảng cáo, chưa có benchmark độc lập `(chưa xác minh)` |
| Lưới an toàn | Test toàn workspace trong CI; hook `.claude/hooks/check-on-edit.py` chạy `cargo check`/`tsc` sau mỗi lần Edit và trả lỗi về cho agent | Bắt phần còn sót | ✅ S0 |

**Obsidian** hợp với tài liệu và bộ nhớ (`memory/` đã tương thích, mở là thấy graph liên kết giữa các trang), nhưng không phân tích được code. Quan hệ trong code dùng LSP và compiler.

Nguồn: [lsp-tools](https://github.com/zircote/lsp-tools), [claude-code-lsps](https://github.com/boostvolt/claude-code-lsps), [Serena discussion](https://github.com/oraios/serena/discussions/1885), [codegraph alternatives](https://tokenade.net/en/articles/codegraph-alternatives), [Codebase-Memory paper](https://arxiv.org/html/2603.27277).
