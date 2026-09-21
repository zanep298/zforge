# Checklist test cho reviewer

## Baseline đã có executable tests

| Hành vi | Test location |
|---|---|
| Store mới rỗng, state tách biệt giữa instances | `tests/test_domain.py` |
| Không sửa state qua object trả về, invalid create không tiêu thụ ID | `tests/test_domain.py` |
| Health, create → read → list, Unicode/trim/default status | `checks/contract.py` |
| Status done, JSON/payload/title/status/field không hợp lệ | `checks/contract.py` |
| Content type, body quá lớn, route/task/query không hỗ trợ | `checks/contract.py` |
| 12 create requests đồng thời không mất task hoặc trùng ID | `checks/contract.py` |
| Runner giữ exit code tests, cleanup đúng scope cả khi timeout, project name riêng | `tests/test_runner.py` (mock Docker) |

HTTP contract chạy hai lần: local server riêng mỗi test trong container checks,
rồi qua network tới container API. Đừng cộng hai lần chạy thành hai loại bằng
chứng độc lập; chúng dùng cùng bộ assertions.

## Khi hoàn thành task mới

- Map từng acceptance criterion sang test mới và kết quả actual.
- Chứng minh tests mới fail vì thiếu feature trên baseline, không phải do lỗi
  environment/import; sau thay đổi chúng phải pass.
- Baseline tests vẫn pass, không bỏ assertion để tạo kết quả xanh.
- Báo tree/commit, image digest, lệnh, exit code và log của lần kiểm tra.
- Giải thích 3–5 dòng về cách làm, phần chưa làm và rủi ro còn lại.
- Review test cases và test code liên quan; vẫn cần targeted code review cho
  thay đổi về concurrency, state, authorization hoặc risk cao.

Acceptance criteria trong task briefs chưa phải tests đã chạy. Một log tests xanh
không chứng minh agent độc lập review, model chọn đúng hay engine recovery đúng.
