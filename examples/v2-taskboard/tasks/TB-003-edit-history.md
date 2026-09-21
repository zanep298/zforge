# TB-003 — Sửa task và xem lịch sử thay đổi

## Mong muốn cuối cùng

Cho phép đổi title/status, tránh ghi đè khi hai người sửa cùng lúc, và đọc lịch sử
thay đổi của từng task. Mọi dữ liệu vẫn synthetic/in-memory; không thêm database,
auth, frontend hoặc production integration.

**Lần chạy đầu chỉ yêu cầu plan-only.** Không implement các phase chỉ vì chúng
được mô tả ở đây. Output phải là plan/dependency/test cases và các điểm cần chốt.

## Phân phase đề xuất để agent đánh giá

1. PATCH title/status, validate trước khi ghi, update atomic; giữ GET/POST hoạt động.
2. Revision và optimistic concurrency, stale write không làm thay đổi state.
3. Append-only history của các update thành công, endpoint đọc lịch sử.

Mỗi phase cần acceptance criteria cụ thể, contract/error shape, tests và điều kiện
handoff. Plan phải chỉ ra phase nào phụ thuộc phase nào và cách tránh phase trước
tạo API khó bổ sung concurrency/history. Agent được đề xuất cách chia khác kèm lý do.

## Các quyết định cần chốt ở plan

- PATCH empty/unknown fields, title/status validation và response shape.
- Revision truyền ở header hay JSON, giá trị khởi đầu, conflict status/code.
- No-op update có tăng revision/ghi history không.
- History có tính create event không; ordering và nội dung before/after.

Đưa recommendation và tradeoff ngắn; chỉ hỏi người dùng khi thiếu lựa chọn thực
sự ảnh hưởng contract. Không tự suy diễn có tài khoản/identity đáng tin cậy.

## Khi được giao phase 1 riêng

Chỉ implement phase 1 theo plan đã chốt. Cung cấp tests success/invalid/not-found,
atomic rejection và regression; liệt kê rõ phase 2/3 chưa làm và scope nối tiếp.
Không đánh dấu toàn TB-003 hoàn thành khi mới hoàn thành phase 1.
