# TB-001 — Lọc task theo trạng thái

## Mong muốn

Người dùng muốn xem riêng việc đang mở hoặc đã xong. Thực hiện feature đầy đủ,
tests và document ngắn. Không thêm pagination hoặc thay persistence.

## Acceptance criteria cần bổ sung tests

- `GET /tasks?status=open` chỉ trả task open; `status=done` chỉ trả task done.
- Không có query giữ nguyên hành vi baseline, ID tăng dần.
- Không có match trả `200 {"tasks":[]}`.
- Status sai, rỗng hoặc lặp nhiều lần trả `400 invalid_status_filter`.
- Query key khác vẫn trả `400 unsupported_query`.
- Filtering không thay đổi dữ liệu và không thay đổi create/get.

## Scope gợi ý

`taskboard/features/listing.py`, test mới `tests/test_status_filter.py`, HTTP/E2E
test mới tương ứng. Bổ sung document riêng `docs/status-filter.md`.
Nếu cần đổi shared router/store, giải thích dependency trước khi chạy batch.

Reviewer cần xem bảng test cases, bằng chứng baseline fail/new tree pass và
giải thích cách validate query. Không claim hoàn thành chỉ bằng unit tests filter.
