---
id: TASK-008
parent: MOC-C
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009]
depends_on: [TASK-005, TASK-006, TASK-007]
---

# TASK-008 — Nghiệm thu đầu cuối Mốc C

## Mục tiêu

Chứng minh năm dấu hiệu thành công của 01-outcome bằng test đầu cuối qua binary thật, stub claude, trong git repo tạm.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-005, TASK-006, TASK-007.
- `tests/run_test.rs`, `tests/mcp_v15_test.rs` (cách dựng handover và stub).

## Output

- `tests/feature_run_test.rs`: một kịch bản cho mỗi dấu hiệu thành công.
- Cập nhật `docs/v1.5/usage.md`, `docs/v1.5/workflow.md` (tiến độ Mốc C), `CLAUDE.md`.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Không gọi model thật (người dùng chọn đánh giá từ log sau).
- Working tree chính của repo tạm không đổi sau mỗi kịch bản (so `git status`).

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn cách stub agent tạo file và commit, miễn đi qua `zforge` binary thật.

## Acceptance và kiểm chứng

- AC-01: A ← B ← C: một lệnh, B từ commit của A, C từ commit của B, tích hợp pass, knowledge `integration_verified`.
- AC-02: A blocked → B, C không chạy; D verified.
- AC-03: Kill giữa B, chạy lại → A không chạy lại; B có run interrupted và run mới.
- AC-04: Amendment sửa C, handover mới → A, B tái dùng, chỉ C chạy.
- AC-05: Mọi task pass, tích hợp fail → không `integration_verified`.
- Toàn bộ `cargo test`, clippy, fmt.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Một dấu hiệu thành công không kiểm được bằng stub.

## Câu hỏi còn mở
