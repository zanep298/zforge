---
id: TASK-002
parent: MOC-C
requirements: [REQ-002, REQ-003]
depends_on: [TASK-001]
---

# TASK-002 — Điểm xuất phát từ output của dependency

## Mục tiêu

Task có dependency chạy được bằng `zforge run <HANDOVER> --task <T>`: worktree xuất phát từ output đã niêm phong của các dependency, và bị từ chối khi dependency chưa verified.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-001 (`Verified.commit`).
- `src/run/contract.rs` (lệnh từ chối `depends_on`), `src/run/worktree.rs`, `src/run/execute.rs::create`.

## Output

- `RunMeta.start: Option<Start>`; `Start { commit, from: Vec<Source> }`, `Source { task, run, commit }`.
- `run::start::plan(handover, task, runs)`: không dependency → baseline (không ghi `start`); một → commit của run verified mới nhất của nó; nhiều → merge.
- `run::start::create`: worktree tại commit đầu, `git merge --no-ff` các commit còn lại; xung đột → `merge --abort`, run `failed` với task và file xung đột, không gọi agent.
- Bỏ từ chối `depends_on` trong `contract.rs`; `execute::create` từ chối khi dependency không có run verified có `commit` trong cùng handover, nêu dependency nào và vì sao (chưa chạy / chưa verified / verified trước khi có commit).

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Run không dependency ghi `run.yaml` giống hệt Mốc B.
- Không đụng checkout chính: merge chỉ xảy ra trong worktree của run.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn thứ tự merge (khuyến nghị theo thứ tự manifest) miễn ổn định.

## Acceptance và kiểm chứng

- AC-01: B phụ thuộc A; A verified → run B có `start.commit` = commit của A, `start.from` = [A, run của A]; file của A có trong worktree B.
- AC-02: A chưa có run verified → `zforge run H --task B` từ chối, không tạo run, lỗi nêu A.
- AC-03: E phụ thuộc B và D (không xung đột) → worktree E chứa file của cả hai; `start.from` có hai nguồn.
- AC-04: B và D sửa cùng dòng → run E `failed`, lý do nêu B, D và file xung đột; không có `attempt_started`.
- AC-05: Task không dependency: `run.yaml` không có `start`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Merge nhiều dependency cần chiến lược khác merge commit (rebase, octopus).
- Output của dependency phải bao gồm cả thứ ngoài git.

## Câu hỏi còn mở
