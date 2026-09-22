---
id: TASK-007
parent: MOC-B
requirements: [REQ-011]
depends_on: [TASK-004]
---

# TASK-007 — Knowledge verified và báo cáo kết quả

## Mục tiêu

Knowledge index ghi requirement là `verified` khi và chỉ khi có một lần chạy verified cho task phục vụ nó, kèm run và candidate; kết quả của run có view đọc được.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-004; `src/intake/knowledge.rs`.

## Output

- `Implementation::Verified { run, candidate }` trong index, đọc từ record của run.
- `result.md` của run: task, hợp đồng đã dùng (file/revision/hash), trạng thái, các lần verify, chi phí, branch.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- Không suy ra `verified` từ nội dung do agent viết.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.

## Acceptance và kiểm chứng

- AC-01: Có run `verified` cho TASK-x → mọi REQ của TASK-x là `verified (RUN-n, candidate …)`; run `failed`/`blocked` không đổi trạng thái.
- AC-02: Requirement `superseded` không bao giờ hiện `verified`.
- AC-03: Sửa `result.md` bằng tay không đổi index hay trạng thái run.
- Test e2e: handover → run với stub tới verified → `zforge knowledge index --json`; toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải thêm trạng thái `integrated` ở mốc này.

## Câu hỏi còn mở
