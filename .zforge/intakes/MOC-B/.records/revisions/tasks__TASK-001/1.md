---
id: TASK-001
parent: MOC-B
requirements: [REQ-003, REQ-007]
depends_on: []
---

# TASK-001 — Record của run

## Mục tiêu

Một lần chạy có record bền: `run.yaml` ghi một lần, `events.jsonl` append-only, và trạng thái §6.1 được phát lại từ sự kiện; lần chạy có worker đã chết được nhận ra là `interrupted`.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- `src/intake/record.rs` (mẫu log append-only + bỏ dòng cuối bị cắt), `src/job/lifecycle.rs::reconcile_dead_worker` (mẫu reconcile).

## Output

- Module `src/run/record.rs`: `RunMeta` (run.yaml), `RunEvent`, `append`, `read`, `state() -> RunState` (trạng thái, chi phí đã dùng, lần verify gần nhất, lý do dừng).
- `src/run/reconcile.rs`: `running`/`verifying` với PID worker đã chết → ghi `failed` lý do `interrupted`.
- Cấp phát ID `RUN-nnn` không trùng dưới lock.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- Trạng thái kết thúc (`verified`, `failed`, `cancelled`, `blocked`) không bao giờ bị sự kiện sau đổi lại.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.

## Acceptance và kiểm chứng

- AC-01: Phát lại chuỗi sự kiện ra đúng trạng thái §6.1 cho mọi chuyển tiếp hợp lệ, và sự kiện không hợp lệ (ví dụ `verified` sau `cancelled`) bị từ chối khi ghi.
- AC-02: Dòng cuối bị cắt dở được bỏ qua; dòng hỏng ở giữa là lỗi, không bị bỏ qua lặng lẽ.
- AC-03: `run.yaml` đã tồn tại thì không ghi đè; hai lần tạo run đồng thời nhận hai ID khác nhau.
- AC-04: Run `running` có PID đã chết được reconcile thành `failed (interrupted)`; PID còn sống thì giữ nguyên.
- Unit test cho từng AC; chạy `cargo test run::record run::reconcile` và toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải đổi máy trạng thái §6.1 hoặc thêm trạng thái mới.
- Phải lưu trạng thái ngoài `events.jsonl`.

## Câu hỏi còn mở
