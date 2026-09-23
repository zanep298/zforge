---
id: TASK-006
parent: MOC-C
requirements: [REQ-006]
depends_on: [TASK-005]
---

# TASK-006 — Tái dùng task qua handover

## Mục tiêu

Ở handover mới, task có hợp đồng và nền code không đổi được tái dùng từ run verified ở handover trước thay vì chạy lại, và việc tái dùng được ghi lại.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-005; manifest cũ và mới của cùng intake (`intake::handover::list`).

## Output

- `run::reuse::contract_key(manifest, task, deps)`: SHA-256 của hash 4 stage, hash file task, hash file task của mọi dependency bắc cầu, commit baseline.
- `run::reuse::find`: run `verified` có `commit` ở handover trước của cùng intake, cùng khóa.
- `RunEvent::Reused { from_run, from_handover, candidate, commit }`: `ready` → `verified`, cost 0.
- Vòng chạy feature tái dùng trước khi tạo run mới; in `A reused RUN-001 (HANDOVER-001)`; `FeatureState` là `Reused`.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Trùng tuyệt đối mới tái dùng; không có ngưỡng gần giống.
- Không tái dùng khi run gốc là tích hợp.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự quyết định có cho `--no-reuse` hay không; nếu có, ghi lý do.

## Acceptance và kiểm chứng

- AC-01: Sửa và chốt lại chỉ C, handover mới → A, B, D `reused`, C chạy từ commit mà B tái dùng; tích hợp chạy.
- AC-02: Sửa 03-solution → không task nào tái dùng.
- AC-03: Handover mới trên baseline commit khác → không task nào tái dùng.
- AC-04: Sửa A → A, B, C chạy lại (B, C phụ thuộc A), D tái dùng.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Cần tái dùng khi hợp đồng chỉ khác về hình thức (khoảng trắng, typo).

## Câu hỏi còn mở
