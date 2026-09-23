---
id: TASK-007
parent: MOC-C
requirements: [REQ-009]
depends_on: [TASK-003]
---

# TASK-007 — Knowledge ba mức triển khai

## Mục tiêu

Knowledge index phân biệt requirement đã verified trên branch riêng, feature đã qua kiểm chứng tích hợp, và feature đã nằm trong branch baseline — tất cả suy từ record và git.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-003 (run tích hợp có commit).
- `src/intake/knowledge.rs` (đã có `verified`).

## Output

- `Implementation::IntegrationVerified { run, candidate }` cho requirement thuộc task trong handover có run tích hợp verified.
- `Implementation::Integrated { commit }` khi commit của run tích hợp là tổ tiên của branch baseline hiện tại (`git merge-base --is-ancestor`).
- Mức cao nhất đúng được hiển thị; `index.json` giữ đủ run và commit để truy vết.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1 hay Mốc B.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- `run.yaml` và `events.jsonl` của Mốc B vẫn đọc được; trường mới chỉ ghi khi có giá trị.
- Không lưu `integrated`; mỗi lần dựng index tính lại.
- Không có git hoặc baseline không tồn tại → không bao giờ `integrated`.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn cách trình bày trong `index.md`.

## Acceptance và kiểm chứng

- AC-01: Run tích hợp verified → requirement liên quan `integration_verified`.
- AC-02: Merge branch tích hợp vào baseline (fast-forward hoặc merge commit) → `integrated`.
- AC-03: Squash merge → vẫn `integration_verified`, không `integrated`.
- AC-04: Run tích hợp failed → requirement giữ `verified` của task.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Cần người dùng ghi nhận `integrated` thay vì suy từ git.

## Câu hỏi còn mở
