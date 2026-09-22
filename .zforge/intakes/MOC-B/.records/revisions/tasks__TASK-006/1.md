---
id: TASK-006
parent: MOC-B
requirements: [REQ-010]
depends_on: [TASK-004]
---

# TASK-006 — Amendment trong lúc chạy

## Mục tiêu

Khi agent kết luận hợp đồng phải sửa, nó ghi change request và lần chạy dừng `blocked` lý do amendment; hợp đồng đã chốt không bị sửa bởi lần chạy.

## Input

- Bản đã chốt của 01-outcome, 02-behavior, 03-solution, 04-breakdown (theo manifest).
- Output TASK-004; workflow §8 (nội dung change request).

## Output

- Template `changes/CHANGE-nnn.md` theo §8 (hợp đồng đang áp dụng, bằng chứng, đề xuất, tác động, diff cần quyết định).
- Cơ chế agent báo cần amendment (ví dụ tạo file change request trong thư mục của intake) và runtime chuyển run sang `blocked` với tham chiếu CHANGE.
- `zforge intake status` liệt kê change request đang mở.

## Ràng buộc

- Toàn bộ test hiện có pass; không sửa kỳ vọng của test v1.
- `cargo clippy --all-targets -- -D warnings` và `cargo fmt --check` sạch.
- Không thêm dependency mới nếu không ghi lý do trong báo cáo.
- Không có lệnh chốt change request ở mốc này; sửa hợp đồng đi qua review/accept/handover mới.

## Tự chủ

- Tự chọn cấu trúc module, tên hàm, cách viết test trong phạm vi task.
- Tự chẩn đoán và sửa lỗi test/clippy trong phạm vi task.
- Tự chọn cách agent báo hiệu amendment, miễn runtime nhận biết được mà không dựa vào lời văn tự do.

## Acceptance và kiểm chứng

- AC-01: Stub agent tạo change request → run `blocked`, lý do `amendment: CHANGE-nnn`, không có lần gọi agent tiếp theo.
- AC-02: Change request được linter kiểm đủ các mục §8.
- AC-03: Snapshot của hợp đồng mà run dùng không đổi sau khi run blocked (hash như manifest).
- Test e2e với stub; toàn bộ `cargo test`.

## Bàn giao

Thay đổi local trên branch của task và báo cáo kiểm chứng (lệnh, kết quả, candidate); không tự push hoặc merge.

## Cần amendment khi

- Phải có lệnh chốt change request ngay ở mốc này.
- Phải cho run tự tiếp tục với hợp đồng mới.

## Câu hỏi còn mở
