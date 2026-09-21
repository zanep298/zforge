# zForge v1.5 — Hiểu, chốt và giao việc cho agent

**Trạng thái: đề xuất thiết kế, chưa phải workflow đã được implement.**

V1.5 là một hướng phát triển riêng so với [v2](../v2/README.md). Trọng tâm là
giúp người sở hữu sản phẩm hiểu và kiểm soát công việc qua intake top-down,
sau đó giao cho agent tự thực hiện theo hợp đồng đã thống nhất.

Tên `v1.5` xác định nhánh tài liệu và hướng sản phẩm; chưa xác định package
version, lịch phát hành hoặc quan hệ kế thừa runtime với v1/v2.

## Mục tiêu

- Người dùng hiểu mục tiêu, hành vi, quyết định quan trọng và cấu trúc công việc.
- Mỗi mức intake tạo file dễ đọc để người dùng review và chốt trước khi đi sâu.
- Task nhỏ nhất có đủ input, output, ràng buộc và tiêu chí kiểm chứng để implement.
- Sau bàn giao, agent tự xử lý vấn đề trong hợp đồng, không hỏi lại chi tiết đã giao.
- Các file đã chốt trở thành nguồn product knowledge có phiên bản và nguồn gốc.

## Workflow

```text
Product knowledge hiện hành + yêu cầu mới
    → Làm rõ mục tiêu
    → Làm rõ hành vi
    → Làm rõ giải pháp
    → Phân rã phase/task
    → Chốt hợp đồng task nhỏ nhất
    → Kiểm tra readiness và bàn giao
    → Agent implement, tự sửa, kiểm chứng và tích hợp
    → Ghi nhận kết quả và cập nhật bằng chứng trong knowledge
```

Người dùng đồng hành và quyết định trong intake. Agent tự chủ trong implementation
theo đúng hợp đồng được bàn giao. Khi cần thay đổi hợp đồng, chỉ phần bị ảnh hưởng
quay lại intake; các phần độc lập có thể tiếp tục.

## Tài liệu chính

Đọc [Workflow intake, thực thi và product knowledge](./workflow.md) để xem:

- trách nhiệm của người dùng, agent và runtime;
- các stage và file đầu ra;
- tiêu chuẩn task đủ để implement;
- review, chốt phiên bản và xử lý thay đổi;
- tự phục hồi và điều kiện quay lại intake;
- vòng đời product knowledge;
- ví dụ task, lộ trình MVP và kịch bản nghiệm thu.

## Fix bugs và cải tiến

Hai lượt review runtime và init/tool ngày 16–18/09/2026 được đưa vào backlog
v1.5. Đây là công việc cần thực hiện, chưa phải các lỗi đã sửa hoặc capability
đã được triển khai.

| Tài liệu | Nội dung |
|---|---|
| [Backlog sửa lỗi](./bug-fixes.md) | 17 nhóm lỗi runtime và init/dispatch; mức ưu tiên, bằng chứng, vị trí code, hướng sửa và tiêu chí nghiệm thu |
| [Kế hoạch cải tiến](./improvements.md) | 6 nhóm cải tiến về operation, persistence, CI, native skills/agents/MCP, readiness tool và trace/benchmark |

Ưu tiên kết quả/gate chính xác, state/lock/timeout an toàn và init đúng client.
Các lỗi P1 trên đường chạy được chọn phải được xử lý trước khi nghiệm thu thực
thi tự chủ ở Mốc B. Thiết kế intake ở Mốc A có thể tiếp tục song song. Mỗi mục
có ID ổn định để đưa vào hợp đồng task và theo dõi bằng chứng hoàn thành.

## Quan hệ với các hướng khác

[V1](../v1/workflow.md) mô tả pipeline hiện có. Tài liệu v2 mô tả một hướng thiết
kế khác. V1.5 có thể tham khảo các cơ chế đã có, nhưng các ADR của v2 không tự
động trở thành quyết định của v1.5. Backend lưu trữ, schema, isolation adapter,
CLI/MCP và chiến lược tương thích phải được quyết định riêng khi triển khai.

Phạm vi tài liệu này là chuẩn bị yêu cầu, thực hiện thay đổi trong repository,
kiểm chứng và bàn giao kết quả. Merge, publish hoặc deployment chỉ được thực
hiện khi có phạm vi và quyền được giao rõ ràng; tài liệu này không tự cấp quyền đó.
