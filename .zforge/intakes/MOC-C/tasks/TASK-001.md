---
id: TASK-001
parent: MOC-C
requirements: [REQ-002]
depends_on: []
---

# TASK-001 — Niêm phong output của run verified

## Mục tiêu

Khi test pass, output của run trở thành một commit cố định trên branch của run, ghi trong event `verified`, với fingerprint bằng đúng candidate vừa test.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- `src/run/execute.rs` (chỗ ghi `Verified`), `src/run/ops.rs::commit_leftovers`, `src/evidence/`.

## Output

- `RunEvent::Verified` thêm `commit: Option<String>`.
- `run::output::seal`: commit thay đổi còn lại vào branch của run, tính lại fingerprint, so với candidate; trả về commit.
- `execute` gọi `seal` trước khi ghi `verified`; seal lỗi hoặc fingerprint lệch → run `failed` với lý do, không `verified`.
- Hàm git dùng chung cho `seal` và `commit_leftovers` (không chép lần ba).

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Không đổi candidate đã test: fingerprint sau khi commit phải bằng candidate, kiểm bằng code, không giả định.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn nội dung commit message, miễn nêu run và task.

## Acceptance và kiểm chứng

- AC-01: Stub agent để lại file chưa commit, test pass → `verified` có `commit`; `git show <commit>` chứa file đó; fingerprint của commit bằng candidate.
- AC-02: Agent đã tự commit hết → `seal` không tạo commit rỗng; `commit` là HEAD của branch.
- AC-03: Commit do agent tạo sau lần verify cuối (nếu có) không phải output: `commit` là cái đã niêm phong.
- AC-04: `events.jsonl` Mốc B (không có `commit`) vẫn phát lại được.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Fingerprint của worktree thay đổi khi commit (giả định ở 03-solution sai).
- Phải niêm phong bằng cách khác commit (tag, ref riêng).

## Câu hỏi còn mở
