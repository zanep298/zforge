---
id: TASK-008
parent: MOC-B
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006]
depends_on: [TASK-004, TASK-005, TASK-006, TASK-007]
---

# TASK-008 — Nghiệm thu bằng benchmark thật

## Mục tiêu

Chứng minh Mốc B trên Claude thật: một leaf task của một intake đã chốt đi từ handover tới `verified` trong worktree riêng, có budget, với trace và evidence truy vết được.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-004 → TASK-007; `tests/bench_claude_test.rs` (benchmark IMP-006).

## Output

- Benchmark `#[ignore]` mới: dựng project tạm, intake nhỏ đã chốt qua thư viện, handover, `zforge run`, rồi kiểm tra kết quả; lưu trace + record vào `target/bench/`.
- Ghi kết quả lượt chạy (model, chi phí, số vòng, finding) vào `docs/v1.5/`.

## Ràng buộc

- Budget mỗi lần chạy benchmark ≤ $1.00; model mặc định haiku.
- Tiêu chí pass về setup/hệ thống, không về gu của model (như benchmark IMP-006).

## Tự chủ

- Tự chọn kịch bản và ngôn ngữ của project tạm; nên chọn ngôn ngữ CodeGraph hỗ trợ.

## Acceptance và kiểm chứng

- AC-01: Benchmark pass: run `verified`, working tree chính không đổi, branch có commit/thay đổi của agent, candidate trong sự kiện bằng fingerprint của worktree.
- AC-02: Không có finding infrastructure trong trace; finding agent choice được báo cáo.
- AC-03: Tổng chi phí ≤ budget của manifest.
- `cargo test --test <benchmark> -- --ignored --nocapture`; lưu báo cáo; toàn bộ `cargo test` không chạy benchmark.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải nâng budget benchmark trên $1.00 mỗi lượt.
- Tiêu chí pass phải dựa vào đánh giá chất lượng của model.

## Câu hỏi còn mở
