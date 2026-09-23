---
id: TASK-005
parent: MOC-C
requirements: [REQ-001, REQ-005, REQ-008]
depends_on: [TASK-004]
---

# TASK-005 — Vòng chạy cả feature

## Mục tiêu

`zforge run <HANDOVER>` chạy mọi task theo thứ tự rồi tích hợp, tiếp tục đúng chỗ sau khi bị ngắt, không bao giờ chạy trùng, chạy được trong nền và qua MCP.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-004 (`FeatureState`), TASK-003 (`integrate`).
- `src/run/ops.rs` (nền, cancel, refresh), `src/cli/run.rs`, `src/mcp/v15.rs`, `src/process.rs::catch_interrupts`.

## Output

- `run::feature_ops::run_feature`: theo thứ tự manifest, mỗi task: bỏ qua nếu verified; bỏ qua nếu bị chặn; run gần nhất interrupted/failed → retry (`retry_of`); còn lại → tạo run (TASK-002) và `execute`. Xong mọi task → tích hợp. Tóm tắt cuối; exit 0 chỉ khi tích hợp verified.
- Khóa `flock` theo (handover, task) giữ suốt một run (cả đường `--task`), và khóa handover cho vòng chạy; khóa bị giữ → từ chối nêu pid.
- `--async`: worker ẩn `zforge run-worker --handover`; pid và log ở `.zforge/runs/features/<INTAKE>/<HANDOVER>/`.
- `run cancel|log|wait <HANDOVER>`; cancel dừng worker vòng chạy và run đang chạy (run ghi `cancelled`).
- MCP: `run_start` không có `task` → chạy feature trong nền; `run_status`, `run_cancel`, `run_log` nhận id handover. `FORBIDDEN` không đổi.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Không có file trạng thái feature; thư mục `features/` chỉ chứa pid và log.
- Chạy tuần tự; không song song.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự quyết retry run `failed` (không phải interrupted) hay coi là chặn; ghi lựa chọn vào báo cáo, mặc định an toàn là coi `failed` là chặn và chỉ retry `interrupted`.

## Acceptance và kiểm chứng

- AC-01: A ← B ← C + D, stub pass → bốn run task + một run tích hợp, exit 0.
- AC-02: A blocked → B, C không có run; D verified; không có run tích hợp; exit 1 với tóm tắt.
- AC-03: Kill worker lúc B chạy; chạy lại lệnh → không có run mới của A; B có run mới `retry_of`; tiếp tục tới tích hợp.
- AC-04: Hai `zforge run H` cùng lúc → cái thứ hai bị từ chối; `zforge run H --task B` trong lúc vòng chạy đang làm B → từ chối.
- AC-05: MCP `run_start` không task, rồi `run_status` với id handover, rồi `run_cancel`; test transport như `tests/mcp_v15_test.rs`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Cần chạy song song để đạt mục tiêu.
- Cần một nguồn trạng thái ngoài record các run.

## Câu hỏi còn mở
