# TB-002 — Giới hạn độ dài title

## Mong muốn

Giữ tiêu đề gọn: tối đa 120 Unicode code points sau khi trim. Thực hiện feature,
tests và document ngắn. Không giới hạn theo UTF-8 bytes hoặc grapheme clusters.

## Acceptance criteria cần bổ sung tests

- 1 và 120 code points sau trim được chấp nhận.
- 121 code points trả `400 title_too_long`; không tạo record/tiêu thụ ID.
- Unicode dùng cùng quy tắc, ví dụ 120 chữ `ế` được chấp nhận.
- Whitespace trước/sau không tính vào giới hạn; title rỗng vẫn `invalid_title`.
- Không thay đổi status validation, create response hoặc ordering của list.

## Scope gợi ý

`taskboard/features/create.py`, test mới `tests/test_title_limit.py`, HTTP/E2E
test mới tương ứng và `docs/title-limit.md`. Không sửa listing/store nếu không cần.
Chạy độc lập hoặc cùng TB-001 trong batch; mỗi task có test file riêng.

Reviewer cần xem boundary cases 0/1/120/121, Unicode, không ghi state khi reject,
bằng chứng tests mới fail trước thay đổi và toàn suite pass sau thay đổi.
