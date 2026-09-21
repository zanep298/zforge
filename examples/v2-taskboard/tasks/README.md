# Bộ task tham chiếu

Các brief này là đầu vào để thử flow, chưa phải feature đã implement hoặc record
native-v2 đã được schema validation. Mỗi bài bắt đầu từ baseline riêng, trừ khi
ghi rõ integration. Không cần gọi agent trả phí để chạy baseline.

| Scenario | Đầu vào | Cần kiểm chứng khi có engine |
|---|---|---|
| Task đơn | [TB-001](./TB-001-status-filter.md) | Plan, implement, tests, review, handoff |
| Hai task song song | TB-001 + [TB-002](./TB-002-title-limit.md) | Worktree/environment riêng, không ghi chéo, test trên tree tích hợp |
| Task lớn, plan-only | [TB-003](./TB-003-edit-history.md) | Trả plan có dependency và test cases; không sửa app |
| Task lớn, phase-scoped | TB-003, chỉ phase 1 sau khi duyệt plan | Hoàn thành phase được giao, không claim toàn feature đã xong |
| Ngắt và tiếp tục | TB-001, ngắt sau khi có diff nhưng trước verification | Reconcile run, giữ diff, không nhân đôi tác dụng phụ, chạy lại evidence stale |

## Batch TB-001 + TB-002

Hai task cùng xuất phát từ một baseline. TB-001 chủ yếu sửa listing và tests mới;
TB-002 chủ yếu sửa create và tests mới. Nếu plan thực tế cần sửa chung store/router
hoặc cùng document thì scheduler phải phát hiện và serialize/integrate có kiểm soát,
không mặc định chúng độc lập chỉ vì tên task khác nhau.

Sau khi tích hợp, thêm check kết hợp: tạo title hợp lệ ở trạng thái `open` và
`done`, lọc từng trạng thái; tạo title 121 ký tự phải thất bại và không xuất hiện
trong list. Chạy lại toàn baseline + tests hai task trên **tree tích hợp**.
Hai container chạy được không thay cho bằng chứng hai agent task hoàn thành đúng.

## Điều chưa được implement ở đây

Harness điều khiển agent/model, tạo worktree, inject crash, ghi native-v2 records,
independent evaluation và phán quyết acceptance sẽ thuộc backlog zForge v2.
Fixture chỉ cung cấp ứng dụng, môi trường và bài kiểm tra tham chiếu.
