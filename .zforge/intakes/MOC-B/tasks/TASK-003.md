---
id: TASK-003
parent: MOC-B
requirements: [REQ-001, REQ-004, REQ-009]
depends_on: []
---

# TASK-003 — Hợp đồng từ snapshot và Flow::Contract

## Mục tiêu

Một lần chạy nhận hợp đồng đúng như đã chốt: đọc manifest, kiểm hash từng snapshot, dựng prompt code từ snapshot; có `Flow::Contract` cho leaf task; task có dependency bị từ chối.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- `src/intake/handover.rs` (Manifest), `src/intake/record.rs` (snapshot), `src/state/flow.rs`, `templates/code.tmpl`.

## Output

- `src/run/contract.rs`: `load(project_root, handover_id, task) -> Contract` kiểm hash; lỗi nói rõ file/revision lệch.
- `Flow::Contract` (Imported → Coded → Verified) với `next_hint`, flow guard và serde như các flow khác.
- `templates/contract.tmpl`: hợp đồng task nguyên văn + context các stage + phần Verifier Feedback + chỉ dẫn ghi change request.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- Các flow Full/Fixbug/Spike/Docs không đổi hành vi.
- Không đọc file hợp đồng từ working tree hay worktree.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn cách trình bày context trong prompt, miễn hợp đồng task được chèn nguyên văn.

## Acceptance và kiểm chứng

- AC-01: Snapshot khớp → `Contract` có đủ file theo manifest; sửa một byte của snapshot → lỗi nêu tên file và revision, không tạo gì.
- AC-02: Sửa file hợp đồng đang làm việc (không phải snapshot) không làm đổi prompt dựng ra.
- AC-03: Task có `depends_on` không rỗng → lỗi `depends on …; running dependent tasks comes with Mốc C`.
- AC-04: `Flow::Contract` đi Imported → Coded → Verified; `.state.yaml` cũ không có trường flow vẫn đọc ra `Full`.
- Unit test + test với manifest thật tạo bằng thư viện intake; toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải đọc hợp đồng từ nơi khác ngoài `.records/` của checkout chính.
- Phải cho phép task có dependency ở mốc này.

## Câu hỏi còn mở
