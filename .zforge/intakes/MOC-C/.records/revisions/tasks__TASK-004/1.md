---
id: TASK-004
parent: MOC-C
requirements: [REQ-005, REQ-007]
depends_on: [TASK-002, TASK-003]
---

# TASK-004 — Trạng thái feature suy từ record

## Mục tiêu

Trạng thái của cả handover — từng task và bước tích hợp — là một hàm thuần của manifest, dependency và record các run, hiển thị bằng `zforge run status <HANDOVER>`.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-002 (`start`), TASK-003 (`kind`).
- `src/run/record.rs`, `src/run/view.rs`, `src/cli/run.rs`.

## Output

- `run::feature::FeatureState::derive(manifest, deps, runs)`: mỗi task `Waiting { on }`, `Running(run)`, `Verified { run, commit }`, `Reused { run, from }` (dùng ở TASK-006), `Blocked { by }`, `Stopped { run, state }`; và trạng thái tích hợp (`Waiting { on }`, `Running`, `Verified`, `Failed`, `Blocked { by }`).
- Chặn lan truyền: task có dependency bắc cầu `Stopped`/`Blocked` là `Blocked { by }` nêu task gốc gây chặn.
- Nhiều run của một task: run gần nhất quyết định, trừ khi đã có run verified (verified mới nhất thắng).
- `zforge run status <HANDOVER> [--json]` in bảng như 02-behavior tình huống 3; id `RUN-` vẫn là status của một run.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- `derive` không I/O; unit test không cần git.
- Không ghi file trạng thái feature nào.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn định dạng bảng, miễn có task, trạng thái, run, nguồn xuất phát.

## Acceptance và kiểm chứng

- AC-01: Unit test mọi tổ hợp: chờ, đang chạy, verified, A blocked → B, C `Blocked { by: A }`, D không bị ảnh hưởng, tích hợp `Blocked`.
- AC-02: Task có run failed rồi run verified → `Verified`.
- AC-03: `run status HANDOVER-001 --json` khớp `derive` trên cùng record.
- AC-04: `run status RUN-001` giữ nguyên hành vi Mốc B.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Cần lưu trạng thái feature ra file để đủ nhanh.

## Câu hỏi còn mở
