# MOC-C — Outcome

## Vấn đề

Mốc B chạy được một leaf task độc lập: một handover, một task, một worktree từ
commit baseline. Một feature thật thường là nhiều task nối nhau — task sau cần
code task trước đã làm — và hiện:

- task có `depends_on` bị từ chối ngay (`contract.rs`: "running dependent tasks
  comes with Mốc C");
- mỗi run tạo worktree từ commit baseline, nên dù task trước đã `verified`, task
  sau cũng không thấy output của nó;
- không có gì kiểm chứng cả feature: các task pass riêng lẻ trên branch riêng,
  mục "Kiểm chứng tích hợp" trong `04-breakdown.md` không bao giờ được chạy;
- một change request làm dừng một task, nhưng không có gì cho biết task nào
  khác bị ảnh hưởng và task nào vẫn làm tiếp được;
- handover mới sau amendment bắt chạy lại mọi task, kể cả task có hợp đồng
  không đổi và đã verified.

## Người sử dụng

- **Người sở hữu sản phẩm**: bàn giao một feature gồm nhiều task, để agent làm
  hết theo thứ tự, chỉ quay lại khi feature đã kiểm chứng xong, khi hết budget,
  hoặc khi hợp đồng cần sửa.
- **Agent implementation** (Claude headless): làm từng task trên nền output đã
  kiểm chứng của các task nó phụ thuộc.
- **Người review**: thấy feature đã tích hợp và kiểm chứng trên một tree chung,
  truy vết được từng requirement tới task, run và commit.

## Kết quả mong muốn

Một handover gồm nhiều task được thực thi hết bằng một lệnh: các task chạy theo
thứ tự dependency, mỗi task bắt đầu từ output đã verified của các task nó phụ
thuộc, rồi cả feature được kiểm chứng tích hợp trên một candidate chung. Khi
một task bị chặn, các task không phụ thuộc vào nó vẫn chạy xong. Ngắt giữa
chừng rồi chạy lại thì làm tiếp từ chỗ dừng. Sau amendment, handover mới chỉ
chạy lại phần có hợp đồng thay đổi.

## Yêu cầu

- REQ-001: `zforge run <HANDOVER>` không kèm `--task` chạy mọi task của handover theo thứ tự dependency trong manifest, rồi chạy kiểm chứng tích hợp; mỗi task vẫn là một run Mốc B với record, budget và giới hạn vòng riêng.
- REQ-002: Worktree của task có dependency xuất phát từ commit mà run `verified` của từng dependency đã kiểm chứng (candidate đã ghi), không từ baseline; không có dependency thì vẫn từ baseline. Commit xuất phát và nguồn của nó được ghi trong `run.yaml`.
- REQ-003: Task chỉ được chạy khi mọi dependency đã có run `verified` trong cùng handover (hoặc được tái dùng theo REQ-006); chạy riêng một task có dependency chưa verified bị từ chối với lý do nêu rõ dependency nào.
- REQ-004: Kiểm chứng tích hợp chạy mục "Kiểm chứng tích hợp" của `04-breakdown.md` đã pin trên một worktree chứa output verified của mọi task, ghi candidate của tree đó; feature chỉ `verified` khi bước này pass, dù mọi task đều pass.
- REQ-005: Một task `blocked`, `failed` hoặc `cancelled` chặn mọi task phụ thuộc trực tiếp hay gián tiếp vào nó và chặn kiểm chứng tích hợp; các task không phụ thuộc vẫn chạy tiếp tới kết thúc.
- REQ-006: Ở handover mới, một task được tái dùng thay vì chạy lại khi và chỉ khi có run `verified` ở handover trước của cùng intake mà file task đó, bốn stage và mọi dependency bắc cầu của nó được pin với cùng hash; lần tái dùng được ghi lại kèm run gốc.
- REQ-007: Trạng thái của feature (từng task: chờ, đang chạy, verified, tái dùng, bị chặn; và tích hợp) được suy ra từ record của các run; không có file trạng thái feature nào có thẩm quyền.
- REQ-008: Chạy lại `zforge run <HANDOVER>` sau khi bị ngắt làm tiếp từ chỗ dừng: không chạy lại task đã verified, task đang dở được nhận ra là interrupted như Mốc B, rồi tiếp tục; không bao giờ chạy hai run của cùng một task trong một handover cùng lúc.
- REQ-009: Knowledge phân biệt `verified` (trên branch riêng, kèm run và candidate), `integration_verified` (feature pass kiểm chứng tích hợp) và `integrated` (commit đã nằm trong branch baseline); cả ba chỉ suy từ record và git, không từ lời agent.

## Phải giữ nguyên

- Mọi hành vi Mốc B: `zforge run <HANDOVER> --task <T>` cho task không
  dependency, record, budget theo (handover, task), amendment, cancel, retry,
  clean, MCP.
- `run.yaml` cũ (không có trường mới) vẫn đọc được.
- D1: chỉ người ở terminal chốt và handover; thực thi không tạo quyết định.
- Không push, không merge vào baseline, không động vào working tree người dùng.

## Ngoài phạm vi

- Chạy song song các task độc lập (tuần tự trước, §12).
- Model routing thích nghi, UI chuyên dụng.
- Tự merge feature vào baseline; `integrated` chỉ được nhận ra khi người dùng
  đã merge.
- Tự giải quyết xung đột merge giữa output của các dependency.
- Codex và OpenCode làm runner.
- Tổng budget cho cả feature (budget vẫn theo từng task trong handover).

## Dấu hiệu thành công

- Handover ba task A ← B ← C: một lệnh, ba run, worktree của B xuất phát từ
  commit verified của A, của C từ commit verified của B; kiểm chứng tích hợp
  pass; knowledge ghi `integration_verified`.
- A bị `blocked`: B và C không chạy, task D độc lập vẫn chạy tới `verified`.
- Kill giữa lúc chạy B rồi chạy lại lệnh: A không chạy lại, B có run
  interrupted và một run mới.
- Amendment sửa hợp đồng C, handover mới: A và B được tái dùng, chỉ C chạy.
- Test của mọi task pass nhưng kiểm chứng tích hợp fail: feature không
  `integration_verified`.

## Nguồn và knowledge liên quan

- `docs/v1.5/workflow.md` §6.1, §6.3, §8 (amendment), §9 (knowledge), §12 Mốc C,
  §13 (kịch bản "task đã chốt nhưng dependency chưa có output", "tất cả task
  con pass nhưng tích hợp fail", "task verified ở branch riêng").
- `docs/v1.5/decisions.md`: D3, D5, D6.
- Đã có: manifest giữ `tasks` theo thứ tự dependency và readiness đã kiểm vòng
  và dependency ngoài phạm vi (`intake/readiness.rs::dependency_order`);
  `run/` Mốc B; `intake/knowledge.rs` đã có `verified`.
- Intake MOC-B: REQ-009 (từ chối `depends_on`) sẽ bị thay bởi REQ-002/REQ-003
  của intake này.

## Câu hỏi còn mở

- [ ] Task có nhiều dependency xuất phát từ đâu: merge commit verified của các dependency (đúng DAG), hay xếp chồng tuyến tính mọi task trước nó theo thứ tự manifest?
- [ ] Kiểm chứng tích hợp là một run riêng (cùng record, `kind: integration`) hay một bước cuối trong run của task cuối?
- [ ] Tái dùng task ở handover mới (REQ-006) có đáng làm ở mốc này, hay mọi handover mới chạy lại hết?
- [ ] `integrated` suy ra bằng git (`merge-base --is-ancestor` commit với baseline) có đủ, hay cần người dùng ghi nhận?
