---
id: TASK-003
parent: MOC-C
requirements: [REQ-004]
depends_on: [TASK-002]
---

# TASK-003 — Run kiểm chứng tích hợp

## Mục tiêu

Một run `kind: integration` kiểm chứng cả feature trên một tree chứa output của mọi task, bằng lệnh tích hợp đã pin, không gọi agent.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-002 (`run::start`).
- Mục "Kiểm chứng tích hợp" của `04-breakdown.md` đã pin; `src/process.rs::run_bounded`; `src/intake/lint.rs::INTEGRATION`.

## Output

- `RunMeta.kind: task | integration` (mặc định `task`).
- `Contract::integration_commands()`: khối code đầu tiên trong mục "Kiểm chứng tích hợp", mỗi dòng không rỗng là một lệnh; không có khối → `project.test_command`; kết quả ghi rõ nguồn.
- `run::integrate`: tạo run tích hợp với branch `zforge/<INTAKE>/integration/<RUN>`, xuất phát từ baseline merge output của các task lá (task không bị task nào khác trong handover phụ thuộc), chạy lần lượt từng lệnh trong worktree, dừng ở lệnh fail đầu tiên.
- Pass → niêm phong (TASK-001) và `verified` với candidate + commit; fail → `failed` với lệnh fail và phần cuối output.
- Từ chối tạo run tích hợp khi còn task chưa verified.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Không gọi agent, không tốn budget.
- Lệnh chạy có timeout và dừng cả cây process như test v1 (`run_bounded`).

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn timeout mặc định cho lệnh tích hợp (ghi trong báo cáo).

## Acceptance và kiểm chứng

- AC-01: A ← B ← C: worktree tích hợp chứa output của C (một lá, không merge); lệnh pass → `verified` với candidate + commit.
- AC-02: Lá C và D: worktree tích hợp chứa cả hai; `start.from` có C và D.
- AC-03: Lệnh thứ hai fail → run `failed`, lý do nêu lệnh đó; lệnh thứ ba không chạy.
- AC-04: Breakdown không có khối code → chạy `project.test_command`, record ghi nguồn là config.
- AC-05: Một task chưa verified → từ chối, không tạo run.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Tích hợp cần gọi agent để sửa.
- Lệnh tích hợp phải đến từ nơi khác ngoài breakdown đã pin.

## Câu hỏi còn mở
