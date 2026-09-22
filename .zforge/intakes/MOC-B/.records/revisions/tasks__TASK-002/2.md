---
id: TASK-002
parent: MOC-B
requirements: [REQ-002]
depends_on: []
---

# TASK-002 — Worktree của lần chạy

## Mục tiêu

Mỗi lần chạy có một git worktree riêng trên branch `zforge/<task>/<run>` tạo từ commit baseline; worktree được giữ khi chạy hỏng và xóa được khi người dùng muốn.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Commit baseline của manifest; vị trí `.zforge/worktrees/<RUN>` (03-solution).

## Output

- Module `src/run/worktree.rs`: `create(project_root, run, task, commit) -> Worktree`, `remove`, `list`.
- Worktree tạo tại `.zforge/worktrees/<RUN>`; `.zforge/worktrees/` được thêm vào `.gitignore` của project một lần, không trùng, giữ nguyên nội dung còn lại.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- Không đổi branch hiện tại hay working tree của người dùng.
- Không xóa branch khi xóa worktree.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.

## Acceptance và kiểm chứng

- AC-01: Tạo worktree từ một commit cụ thể; `git -C <worktree> rev-parse HEAD` bằng commit đó và branch đúng tên.
- AC-02: `git status` của working tree chính không đổi sau khi tạo và xóa worktree.
- AC-03: Branch đã tồn tại, đường dẫn đã có, hoặc không phải git repo → lỗi cụ thể, không để lại worktree dở.
- AC-04: `remove` xóa worktree nhưng giữ branch.
- AC-05: `.gitignore` có đúng một dòng `.zforge/worktrees/` sau nhiều lần tạo; file `.gitignore` chưa có thì được tạo; nội dung cũ giữ nguyên.
- Test tích hợp trong git repo tạm cho từng AC; toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải dùng cơ chế cách ly khác git worktree.
- Phải push hoặc đổi branch baseline.

## Câu hỏi còn mở
