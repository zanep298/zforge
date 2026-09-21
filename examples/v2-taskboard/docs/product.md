# Product context và API contract

Taskboard phục vụ một nhóm giả lập theo dõi danh sách việc. Baseline chỉ là local
test fixture, không auth, không multi-tenant, không lưu bền và không có giao diện.

## Baseline

| Request | Kết quả |
|---|---|
| `GET /health` | `200 {"status":"ok"}` |
| `POST /tasks` | Body JSON `title`, optional `status`; `201` kèm task |
| `GET /tasks` | `200 {"tasks":[...]}`, ID tăng dần |
| `GET /tasks/{id}` | `200` kèm task hoặc `404 task_not_found` |

Task có `id` nguyên dương duy nhất trong server process, `title` và `status`.
Title phải là string không rỗng sau `strip`; lưu title đã strip và giữ Unicode.
Status mặc định `open`, chỉ chấp nhận `open | done`. Không chấp nhận field thừa.
Baseline **chưa giới hạn 120 ký tự**, **chưa lọc status**, **chưa sửa task**.

Body tối đa 16,384 bytes; vượt giới hạn trả `413 body_too_large`. Body phải có
Content-Type `application/json`, một Content-Length hợp lệ, không dùng chunked
transfer. JSON lỗi trả `400 invalid_json`, loại content khác trả `415 json_required`.
Payload không phải object trả `400 invalid_payload`; title/status sai trả
`400 invalid_title` / `400 invalid_status`; field thừa trả `400 unknown_field`.
Request bị từ chối không được tạo task hoặc tiêu thụ ID.

Query không hỗ trợ trả `400 unsupported_query`; route lạ trả `404 route_not_found`.
PATCH/PUT/DELETE hiện trả `405 method_not_allowed`. Đây là API test nhỏ, không phải
HTTP server đã hardened hay security benchmark toàn diện.

## Conventions và ranh giới

- `features/create.py` sở hữu validation/create; `features/listing.py` sở hữu list.
- `features/detail.py` sở hữu lookup; `store.py` sở hữu dữ liệu và thread lock.
- `server.py` chỉ routing, decode request và encode response.
- Feature mới bổ sung test file riêng để giảm xung đột khi chạy song song.
- Không thêm dependency, service hoặc thay persistence nếu task không yêu cầu.
- Không gọi external API, dùng dữ liệu thật hoặc thêm deployment automation.
- Các task trong `tasks/` mô tả yêu cầu tương lai, không sửa baseline trước khi
  bắt đầu bài tập. Khi tiêu chí thay đổi cần ghi lại phạm vi thay đổi.
