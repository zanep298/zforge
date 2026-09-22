---
id: TASK-005
parent: MOC-B
requirements: [REQ-007, REQ-008]
depends_on: [TASK-001, TASK-004]
---

# TASK-005 — Vận hành lần chạy

## Mục tiêu

Người dùng xem, liệt kê, hủy, chạy lại và dọn các lần chạy; lần chạy nền dùng lại job nên hủy và phát hiện worker chết có sẵn.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-001, TASK-004; `src/job/` (worker, cancel, reconcile), `src/cli/trace.rs` (mẫu hiển thị).

## Output

- `zforge run status <RUN> [--json]`, `run list [--handover H]`, `run cancel <RUN>`, `run retry <RUN>`, `run clean <RUN>`, `zforge run … --async`.
- `retry` tạo run mới pin cùng manifest, ghi `retry_of`, dùng phần budget còn lại (từ chối nếu hết); `clean` chỉ xóa worktree của run đã kết thúc.
- `progress.md`/`result.md` sinh lại từ sự kiện.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `cancel` dừng mọi process của run (như `job cancel`).
- Không xóa branch.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.

## Acceptance và kiểm chứng

- AC-01: `run status` hiển thị trạng thái, sự kiện, các lần verify với candidate, trace từng lần gọi agent, chi phí so với budget; `--json` parse được.
- AC-02: Kill worker của một run nền rồi `run status` → `failed (interrupted)`; `run retry` tạo run mới có `retry_of`, với trần budget là phần còn lại.
- AC-03: `run cancel` trên run nền dừng stub agent và process test, ghi `cancelled`.
- AC-04: `run clean` từ chối run đang chạy; với run đã kết thúc thì xóa worktree, giữ branch.
- Test e2e với binary thật và stub; toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải cho phép tiếp tục một run đã `failed`/`cancelled` thay vì tạo run mới.
- Phải xóa branch.

## Câu hỏi còn mở
