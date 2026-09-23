# Workflow v1.5 — Intake top-down và thực thi theo hợp đồng

**Trạng thái: đề xuất thiết kế của một hướng phát triển riêng so với v2.**

Tài liệu ghi nhận workflow được thảo luận với người dùng: tập trung trách nhiệm
làm rõ vào intake; tạo file để review qua từng mức; bàn giao task đủ cơ sở thực
hiện; tái sử dụng nội dung đã chốt làm product knowledge.

Các tên trạng thái, đường dẫn và record dưới đây là thiết kế đề xuất của v1.5,
không phải cam kết rằng CLI, schema hay runtime hiện tại đã hỗ trợ chúng.

## 1. Vấn đề cần giải quyết

AI có thể tạo task và code nhanh hơn khả năng người dùng tiếp nhận. Với một yêu
cầu được chia thành 5 phase và 50 task, việc đọc toàn bộ prompt, diff và báo cáo
cuối cùng khiến người dùng khó hiểu hệ thống, đánh giá lựa chọn và tiếp quản code.

V1.5 đưa việc xây dựng hiểu biết chung lên trước implementation. Người dùng đi từ
mục tiêu đến hành vi, giải pháp và task nhỏ; tại mỗi mức, một file giải thích điều
đang được quyết định. Khi bàn giao, agent có đủ căn cứ và quyền tự chủ để xử lý.

Thành công bao gồm cả chất lượng kết quả và khả năng người dùng giải thích được
những quyết định chính. Số task, số dòng code hoặc ít câu hỏi riêng lẻ không đủ
để chứng minh workflow hiệu quả.

## 2. Ranh giới trách nhiệm

| Chủ thể | Trách nhiệm |
|---|---|
| Người dùng | Cung cấp mục tiêu, quyết định hành vi và đánh đổi quan trọng, review và chốt file trong intake, quyết định thay đổi hợp đồng khi cần |
| Agent intake | Đọc knowledge và code, xác minh giả định, giải thích dễ hiểu, đề xuất giải pháp, phân rã công việc và chuẩn bị hợp đồng |
| Agent implementation | Tự thực hiện, chẩn đoán, sửa lỗi, kiểm thử và cung cấp bằng chứng trong phạm vi hợp đồng |
| Agent review | Kiểm tra kết quả với yêu cầu và bằng chứng, phát hiện sai lệch; không tự hạ tiêu chí để chấp nhận output |
| zForge runtime | Quản lý revision, ghi nhận quyết định có thẩm quyền, kiểm tra readiness, điều phối dependency, giới hạn thực thi và xác nhận trạng thái |

Đây là các vai trò logic; không bắt buộc mỗi vai trò là một process, model hoặc
subagent riêng. Mức độc lập của review cần được quy định theo loại công việc.

Agent có thể đề xuất, nhưng không tự tạo xác nhận của người dùng. Một dòng
`approved: true` do agent viết hoặc việc người dùng đã mở file không có giá trị
chốt nội dung. Runtime phải ghi nhận quyết định thực tế gắn với đúng revision.

## 3. Hợp đồng là ranh giới tự chủ

Trong workflow này, “đảm bảo output” nghĩa là đáp ứng toàn bộ hợp đồng, gồm:

- input và dependency được phép sử dụng;
- hành vi, artifact và tiêu chí chất lượng bắt buộc;
- quy tắc nghiệp vụ, interface và quyết định phải giữ nguyên;
- phạm vi tác động và quyền được giao;
- ngân sách, giới hạn thử lại và điều kiện dừng;
- bằng chứng kiểm chứng và ranh giới bàn giao.

Agent được tự thay đổi cách thực hiện nếu vẫn giữ hợp đồng. Ví dụ, có thể sửa
thuật toán hoặc cấu trúc nội bộ được giao mà không hỏi lại. Thay đổi định dạng
API đã chốt, bỏ một acceptance criterion hoặc mở rộng quyền là sửa hợp đồng.

Intake phải phân biệt **quyết định bắt buộc** với **cách triển khai gợi ý**.
Gợi ý có thể được agent thay bằng cách tương đương và ghi lý do; quyết định bắt
buộc chỉ thay đổi qua quy trình amendment ở mục 8.

## 4. Luồng tổng thể

```text
Knowledge có nguồn gốc + yêu cầu mới
    ↓
Outcome → Behavior → Solution → Breakdown → Leaf task contracts
    mỗi stage: giải thích → file → review/điều chỉnh → chốt revision
    ↓
Kiểm tra tính đầy đủ, khả thi và điều kiện thực thi
    ↓
Bàn giao bộ hợp đồng đã chốt
    ↓
Task đủ dependency → implement → kiểm chứng → tự sửa/review → verified
    ↓
Tích hợp và kiểm chứng đầu ra tổng thể
    ↓
Bàn giao theo phạm vi được giao + cập nhật bằng chứng knowledge
```

Nếu readiness phát hiện thiếu cơ sở, quay lại đúng stage cần làm rõ trước khi
bàn giao. Trong implementation, phát hiện cần sửa hợp đồng tạo change request;
không tự sửa các file đã chốt để làm cho kết quả hiện tại trở nên hợp lệ.

## 5. Các stage intake và file đầu ra

### 5.1. Nguyên tắc trình bày

Mỗi file cần trả lời bằng ngôn ngữ dễ hiểu:

1. Đang làm rõ phần nào và phần này đóng góp gì vào mục tiêu phía trên?
2. Hành vi hoặc quyết định được đề xuất là gì, với ví dụ cụ thể?
3. Vì sao đề xuất như vậy, đánh đổi và điều chưa chắc chắn là gì?
4. Có điểm nào thực sự cần người dùng quyết định?
5. Revision này thay đổi gì so với bản người dùng đã xem?

Thuật ngữ kỹ thuật phải gắn với ý nghĩa và hệ quả. Recommendation đi kèm điều
kiện áp dụng, lựa chọn thay thế và thông tin có thể làm đổi đề xuất. Người dùng
có thể yêu cầu ví dụ, sơ đồ hoặc thử nghiệm nhỏ trước khi chốt.

File là nơi lưu nội dung cần review; trao đổi có thể diễn ra trong chat/editor.
Agent phải cập nhật kết luận trở lại file để người dùng không cần ghép lịch sử
chat mới hiểu được yêu cầu. Chi tiết hỗ trợ được liên kết riêng khi cần.

### 5.2. Outcome — `01-outcome.md`

Làm rõ vấn đề, người sử dụng, kết quả mong muốn, phạm vi, điều phải giữ nguyên,
điều ngoài phạm vi và dấu hiệu thành công. Ghi nguồn yêu cầu và knowledge liên
quan. Mỗi yêu cầu bắt buộc có ID ổn định để theo dõi xuống các task.

Chốt stage khi người dùng hiểu và đồng ý mục tiêu; các câu hỏi có thể trì hoãn
phải được ghi rõ sẽ giải quyết ở stage nào, không biến mất khỏi hồ sơ.

### 5.3. Behavior — `02-behavior.md`

Mô tả hành vi qua tình huống bình thường, biên, lỗi và hồi phục. Xác định input,
output quan sát được, quy tắc nghiệp vụ và các hành vi cũ cần bảo toàn.

Chốt stage khi các hành vi quan trọng đủ cụ thể để người dùng kiểm tra bằng ví
dụ. Quyết định nghiệp vụ chưa rõ phải được giải quyết trước khi task phụ thuộc
được bàn giao cho implementation.

### 5.4. Solution — `03-solution.md`

Giải thích luồng xử lý, component, dữ liệu, interface và các quyết định thiết kế.
Đánh dấu quyết định bắt buộc và gợi ý triển khai. Ghi các phương án đã cân nhắc,
lý do lựa chọn, giả định và bằng chứng khả thi.

Agent phải đọc code/interface liên quan, kiểm tra công cụ và test hiện có. Giả
định quan trọng chưa được xác minh cần một nghiên cứu hoặc thử nghiệm nhỏ trong
phạm vi được phép. Kết quả thử nghiệm không được tự tính là implementation hoàn
thành; code thử chỉ được giữ lại qua kiểm chứng và phạm vi được giao.

### 5.5. Breakdown — `04-breakdown.md`

Phân rã thành phase và task, giải thích mục đích từng phần, thứ tự, dependency,
input/output giữa các task và các interface dùng chung. Mỗi requirement bắt
buộc phải có task chịu trách nhiệm và chiến lược kiểm chứng.

Mỗi phase có output kiểm tra được. Công việc tích hợp phải có người thực hiện
theo vai trò, input, output và tiêu chí riêng; không được ngầm coi các task con
đã pass là đủ cho phase hoặc feature.

Với 5 phase và 50 task, người dùng review bức tranh 5 phase trước, rồi mở từng
nhánh để đi sâu. Cả bộ task thuộc phạm vi bàn giao phải được chốt và kiểm tra
tính nhất quán trước khi bắt đầu implementation của phạm vi đó.

Có thể bàn giao một phase riêng nếu người dùng chủ động chọn phạm vi đó. Đây
là một lần bàn giao có giới hạn, không phải tuyên bố toàn bộ feature đã sẵn sàng.

### 5.6. Leaf task — `tasks/TASK-001.md`

Task nhỏ nhất cần chứa hoặc tham chiếu chính xác tới:

| Thành phần | Yêu cầu |
|---|---|
| Danh tính | Task ID, revision, parent và requirement IDs |
| Mục tiêu | Một kết quả có thể giải thích và kiểm chứng |
| Input | Knowledge, code baseline, interface, fixture và output phụ thuộc |
| Output | Hành vi và artifact phải tạo, bao gồm tài liệu nếu có ảnh hưởng |
| Ràng buộc | Quy tắc, tương thích, scope và quyết định phải giữ nguyên |
| Tự chủ | Chi tiết agent được tự chọn, công cụ/quyền và giới hạn tài nguyên |
| Acceptance | Tiêu chí bắt buộc và cách kiểm chứng từng tiêu chí |
| Dependency | Điều kiện bắt đầu, interface nhận và cách xác nhận input thực tế |
| Recovery | Cách xử lý trong hợp đồng, giới hạn thử lại và điều kiện dừng |
| Delivery | Output bàn giao ở mức nào và bằng chứng cần kèm theo |

Task đủ nhỏ khi agent có thể thực hiện và kiểm chứng một kết quả mà không cần
thêm quyết định sản phẩm/thiết kế bắt buộc. Không áp một số dòng code hoặc số
file cố định để xác định kích thước task.

Các stage có thể được gộp cho công việc đơn giản nếu nội dung cần thiết vẫn
được review rõ ràng. Việc gộp phải hiển thị trong hồ sơ; không âm thầm bỏ qua
điểm còn cần người dùng quyết định.

## 6. File, revision và readiness

### 6.1. Hai loại trạng thái riêng

Trạng thái tài liệu:

```text
draft → in_review → accepted
             ↘ needs_revision → in_review
accepted → superseded khi một revision thay thế được chấp nhận
```

Chỉnh sửa một file đã chốt tạo revision nháp mới. Bản đã chốt vẫn được giữ để
truy vết; bản nháp không tự thay thế hợp đồng của một run đang thực hiện.

Trạng thái công việc:

```text
planned → waiting_dependencies → ready → running → verifying → verified
                                  running/verifying → blocked | failed | cancelled
                                  verifying → running khi cần sửa trong hợp đồng
```

Task không có dependency có thể đi từ `planned` sang `ready` khi mọi điều kiện
được đáp ứng. `blocked` chỉ quay về `ready` sau khi nguyên nhân được giải quyết
và điều kiện chạy được kiểm tra lại. `failed` hoặc `cancelled` kết thúc run; chạy
lại phải có lần chạy mới và lịch sử rõ ràng.

`accepted` mô tả quyết định về tài liệu. `ready` mô tả điều kiện được phép chạy.
`verified` mô tả kết quả đã kiểm chứng tại một tree/commit cụ thể. Không được
dùng ba trạng thái này thay thế nhau.

### 6.2. Kiểm tra trước bàn giao

Runtime và các bước review phải xác nhận:

- Mọi requirement bắt buộc đều có acceptance criteria và công việc đáp ứng.
- Mọi task đều liên kết về ý định được chấp nhận; không có task tự mở rộng scope.
- Không còn quyết định quan trọng chưa trả lời hoặc giả định trọng yếu thiếu căn cứ.
- Input/output, interface và dependency khớp nhau; đồ thị không có vòng phụ thuộc.
- Có môi trường, quyền, ngân sách và cách kiểm chứng phù hợp cho phạm vi bàn giao.
- Mọi tiêu chí bắt buộc có phương pháp đánh giá; có kiểm chứng tích hợp tổng thể.
- Các file được chốt đúng revision, không có sửa đổi chưa review bị đưa vào run.

Kiểm tra bằng máy xác nhận cấu trúc, reference và điều kiện quan sát được. Review
phải đối chiếu nội dung với ý định gốc; việc file hợp lệ về cấu trúc không chứng
minh yêu cầu đã đầy đủ hoặc thiết kế đúng.

### 6.3. Bộ hồ sơ bàn giao

Một manifest ghi task/phase được giao, revision và hash của file đã chốt,
knowledge snapshot, baseline, dependency/interface, chính sách thực thi và quyền
bàn giao. Thay đổi quyền hoặc ngân sách ngoài phạm vi đã giao là sửa hợp đồng.

Output của task trước có thể chưa tồn tại lúc intake kết thúc. Khi đó, pin hợp
đồng output/interface; lúc task sau chuẩn bị chạy, bind thêm artifact/tree thực
tế đã được kiểm chứng. Output phù hợp hợp đồng không cần người dùng chốt lại.
Output sai hoặc khác interface phải được sửa hoặc xử lý qua amendment.

## 7. Implementation tự chủ

Agent thực hiện theo vòng implement → test/check → diagnosis/correction → review.
Runtime quyết định việc tiếp tục dựa trên kết quả, quyền và giới hạn đã giao.

| Issue | Cách xử lý |
|---|---|
| Lỗi code hoặc test phát hiện sai hành vi | Agent tự sửa và chạy lại kiểm chứng |
| Gợi ý triển khai không phù hợp | Tự chọn cách khác trong hợp đồng, ghi rationale |
| Fixture/test có lỗi | Sửa theo phạm vi được giao; bảo toàn tiêu chí và kiểm tra độc lập thay đổi expectation |
| Lỗi công cụ hoặc môi trường có thể phục hồi | Tự chẩn đoán và phục hồi trong quyền/ngân sách |
| Conflict tích hợp có cách giải quyết bảo toàn hợp đồng | Tự xử lý và chạy lại kiểm chứng trên tree tích hợp |
| Cần bỏ tiêu chí, đổi hành vi/interface hoặc quyết định bắt buộc | Tạo change request, dừng phần bị ảnh hưởng |
| Thiếu quyền/tài nguyên hoặc hết đường xử lý hợp lệ | Block, giữ kết quả và báo nguyên nhân; đề xuất amendment nếu cần thay đổi hợp đồng |

Giới hạn retry phải được xác định trước. Lỗi lặp lại không có bằng chứng mới cần
đổi hướng chẩn đoán hoặc dừng; không được chạy vô hạn hay hỏi người dùng cách
debug từng bước. Hết budget không cho phép tự tăng budget hoặc hạ chất lượng.

Không xóa/hạ assertion, bỏ test bắt buộc hoặc đổi acceptance criteria để có kết
quả xanh. Test mới phải chứng minh hành vi liên quan; với regression, cần chứng
minh phát hiện lỗi cũ khi khả thi. Gate không chạy được phải hiện là thiếu bằng
chứng, không được tính pass.

Agent tự báo tiến trình, quyết định nội bộ và vấn đề có ý nghĩa. Báo cáo không
phải một yêu cầu phê duyệt. Người dùng có thể xem, tạm dừng hoặc hủy công việc,
nhưng không phải xác nhận thường xuyên để agent tiếp tục trong hợp đồng.

Khi code hoặc input thay đổi, kiểm chứng bị ảnh hưởng phải chạy lại. Chỉ kết quả
gắn với đúng candidate mới được dùng để xác nhận output. Feature/phase chỉ được
xác nhận khi output con và kiểm chứng tích hợp đều đạt.

## 8. Thay đổi hợp đồng

Change request có một file review được, chứa:

1. Hợp đồng/revision đang áp dụng và phần cần thay đổi.
2. Bằng chứng mới, vấn đề phát hiện và các cách tự xử lý đã thử.
3. Đề xuất cụ thể, phương án thay thế và hệ quả nếu giữ nguyên.
4. Task, interface, knowledge và kết quả đã làm bị ảnh hưởng.
5. Diff nội dung cần người dùng quyết định.

Người dùng có thể chấp nhận, yêu cầu sửa hoặc từ chối. Agent không diễn giải im
lặng thành đồng ý. Nếu bị từ chối, agent tiếp tục theo hợp đồng cũ khi còn cách
hợp lệ; nếu không, giữ trạng thái blocked hoặc kết thúc failed có giải thích.

Khi được chấp nhận, runtime tạo revision mới, tính lại dependency/readiness và
vô hiệu các bằng chứng bị ảnh hưởng. Run dùng hợp đồng cũ phải dừng phần liên
quan và được rebind có ghi nhận hoặc thay bằng run mới; không đọc file mới như
thể đó là hợp đồng đã dùng từ đầu. Công việc không bị ảnh hưởng được bảo toàn.

## 9. Product knowledge từ hồ sơ đã chốt

### 9.1. Phân biệt ý định và sự thật triển khai

File outcome/behavior/solution đã chốt là nguồn kiến thức về ý định và quyết định.
Task contract lưu lịch sử yêu cầu thay đổi. Báo cáo implementation/test là bằng
chứng về hành vi thực tế tại một phiên bản code.

Mỗi mục knowledge cần hai nhóm thông tin độc lập:

| Nhóm | Ví dụ nội dung |
|---|---|
| Quyết định | Nguồn, revision/hash, người chốt, phạm vi áp dụng, còn hiệu lực hay đã bị thay thế |
| Triển khai | Chưa implement, đã kiểm chứng trên branch/tree, đã tích hợp vào baseline được chỉ định; kèm evidence |

Một quy tắc đã chốt có thể chưa tồn tại trong code. Một task verified trên branch
chưa chứng minh nhánh chính đã có hành vi đó. Knowledge phải giữ rõ các khác biệt
này; không suy ra việc tích hợp chỉ từ trạng thái task hoặc lời agent nói.

### 9.2. Cách cập nhật

MVP dùng index liên kết trực tiếp tới nội dung đã chốt để tránh sao chép và diễn
giải lại. Có thể tự cập nhật reference và bằng chứng trạng thái theo record hợp
lệ. Tổng hợp mới làm thay đổi ý nghĩa, mở rộng phạm vi một quyết định hoặc sửa
quy tắc canonical phải được đưa vào review; không tự trở thành kiến thức tin cậy.

Khi intake mới bắt đầu, agent lấy các mục còn hiệu lực và phù hợp phạm vi, rồi
pin nguồn/revision đã sử dụng. Knowledge là context, không tự cấp quyền thực thi
cho task mới. Thông tin có sẵn cần được đọc trước khi hỏi lại người dùng.

Yêu cầu mới mâu thuẫn với knowledge phải nêu rõ quy tắc cũ và thay đổi được đề
xuất. Revision thay thế giữ liên kết với lịch sử; không xóa nguồn cũ. Knowledge
thay đổi trong lúc run hoạt động cần kiểm tra tác động trước khi áp dụng.

## 10. Tổ chức artifact đề xuất

```text
.zforge/
├── intakes/
│   └── FEATURE-001/
│       ├── 01-outcome.md
│       ├── 02-behavior.md
│       ├── 03-solution.md
│       ├── 04-breakdown.md
│       ├── tasks/
│       │   ├── TASK-001.md
│       │   └── TASK-002.md
│       ├── changes/
│       │   └── CHANGE-001.md
│       └── readiness.md
├── knowledge/
│   └── index.md
└── runs/
    └── RUN-001/
        ├── progress.md
        └── result.md
```

Đây là mặt đọc/review của người dùng, không phải schema lưu trữ runtime hoàn
chỉnh. Cần metadata bền vững cho ID, revision, hash, quyết định, manifest, run,
attempt, dependency và evidence. Cơ chế lưu metadata/history chưa được chọn
trong tài liệu này; các ADR lưu trữ của v2 không tự áp dụng.

File có thể được chỉnh trực tiếp khi intake. Trước khi chốt, runtime phải nhận
đúng nội dung được review và kiểm tra thay đổi. File nháp, record thực thi và
view sinh ra phải có ownership rõ; sửa progress/result không thể tạo test pass
hoặc làm task chuyển trạng thái. CLI, MCP hay UI tương lai dùng chung quy tắc.

## 11. Ví dụ một task đủ để implement

Ví dụ sau minh họa một bản nội dung để review, không phải schema hay quyền đã
được cấp. Các reference chỉ có giá trị khi runtime đã resolve và người dùng chốt.

```markdown
# TASK-002 — Lọc công việc theo trạng thái

Revision: 1
Parent: FEATURE-001
Requirement: REQ-002

## Mục tiêu
Người dùng lấy được danh sách công việc theo trạng thái open hoặc done.

## Input
- Baseline và knowledge revision được ghi trong manifest bàn giao.
- TASK-001 cung cấp API GET /tasks và model trạng thái open | done.
- Output TASK-001 phải được kiểm chứng và bind trước khi bắt đầu.

## Output
- GET /tasks không có filter: giữ hành vi cũ.
- GET /tasks?status=open hoặc status=done: chỉ trả task đúng trạng thái.
- Trạng thái khác, giá trị rỗng hoặc tham số status lặp lại: HTTP 400.
- Có tài liệu ví dụ sử dụng và test cho các trường hợp trên.

## Ràng buộc
- Giữ nguyên cấu trúc JSON, thứ tự danh sách và cách phân quyền hiện có.
- Dữ liệu không bị thay đổi bởi thao tác lọc.
- Không thêm dependency hoặc thay đổi endpoint khác.

## Tự chủ
- Tự chọn cấu trúc hàm và sửa module listing, test, tài liệu liên quan.
- Tự chẩn đoán và sửa trong scope bằng công cụ/giới hạn đã ghi ở manifest.
- Không tự sửa hợp đồng hoặc mở rộng quyền thực thi.

## Acceptance và kiểm chứng
- AC-01: Có cả open/done trong fixture; mỗi filter trả đúng tập task.
- AC-02: Không có status giữ kết quả baseline; filter không có kết quả trả [].
- AC-03: Giá trị không hợp lệ, rỗng và lặp lại trả 400.
- AC-04: Test bảo vệ response shape, thứ tự, phân quyền và dữ liệu không đổi.
- Chạy test hành vi mới và regression liên quan bằng command đã xác minh.
- Ghi candidate tree, command, kết quả và giới hạn; review assertion thực tế.

## Bàn giao
Thay đổi local và báo cáo kiểm chứng; không tự push hoặc merge.

## Cần amendment khi
Phải thay output, response shape, quy tắc phân quyền, dependency hoặc scope.
```

TASK-002 verified không tự làm FEATURE-001 hoàn thành. Công việc tích hợp phải
kiểm tra luồng tạo/đổi trạng thái/lọc trên cùng tree và đúng hợp đồng feature.

## 12. Lộ trình triển khai

### Mốc 0 — Củng cố runtime và tích hợp tool

- Xử lý [backlog sửa lỗi](./bug-fixes.md), ưu tiên P1 về kết quả, gate,
  state/lock, timeout và init đúng target.
- Hoàn thiện [các cải tiến nền tảng](./improvements.md) cần cho đường chạy được
  chọn: operation thống nhất, persistence, regression và adapter client.
- Xác nhận skill/agent/MCP được client nhận và runtime sử dụng đúng; không tính
  việc tạo file cấu hình là bằng chứng execution đã hoạt động.
- Gắn mỗi bản sửa với regression test, candidate và bằng chứng nghiệm thu.

Mốc này là điều kiện của nền tảng được dùng để nghiệm thu Mốc B. Thiết kế và thử
trải nghiệm intake ở Mốc A có thể tiếp tục song song. Việc dùng runtime hiện có
đến mức nào vẫn là quyết định triển khai riêng của v1.5.

### Mốc A — Intake và knowledge có thể sử dụng

- Chốt template, định danh và quan hệ giữa các file.
- Tạo phiên intake, review từng stage, ghi quyết định gắn revision/hash.
- Cho phép revise, xem diff, phát hiện reference cũ và task thiếu requirement.
- Tạo readiness report, manifest bàn giao và knowledge index từ nguồn đã chốt.
- Thử trên một feature thật; xác nhận người dùng hiểu được các quyết định chính.

Mốc này chứng minh trải nghiệm intake, chưa tuyên bố có execution tự chủ.

**Tiến độ (22/09/2026, `src/intake/`, `src/cli/intake.rs`):**

| Hạng mục | Đã có |
|---|---|
| Template, định danh | `zforge intake new <ID>` sinh 4 stage; `zforge intake task <ID> TASK-xxx` sinh leaf task (frontmatter `id/parent/requirements/depends_on` + 8 section §5.6). `REQ-001` định nghĩa trong 01-outcome, `AC-01` trong task, câu hỏi mở `- [ ]` |
| Review và quyết định | `zforge intake review` chụp snapshot, ghi hash, in diff với revision đã chốt; `accept` / `revise --note` chỉ chạy với TTY + gõ xác nhận (D1), chỉ cho đúng hash đang review (D2). Nhật ký append-only + snapshot trong `.records/`; trạng thái suy ra, không lưu trong file. `.claude/settings.json` deny các lệnh này với Claude |
| Revise, diff, reference cũ, task thiếu requirement | Linter: REQ phải được định nghĩa, task phải trỏ tới REQ có thật, section/AC bắt buộc. Readiness cảnh báo file được chốt trước revision mới của file cấp trên |
| Readiness, manifest, knowledge | `zforge readiness` (§6.2, trên bản đã chốt) ghi `readiness.md`; `zforge handover` (TTY) ghi `HANDOVER-nnn.json` với file/revision/hash, thứ tự task, baseline + HEAD + fingerprint, policy, ranh giới bàn giao; `zforge knowledge index` sinh index từ bản đã chốt, tách trạng thái quyết định (active/superseded) và triển khai (not_implemented/handed_over) |
| Agent intake | Skill native `zforge-intake`: thứ tự stage, nội dung từng file, gửi review và không bao giờ tự chốt |

Chưa làm: **thử trên một feature thật** với người dùng (tiêu chí cuối của mốc);
MCP cho intake (D4: CLI trước); change request (§8, `changes/`) mới có thư mục,
chưa có lệnh; ID của quyết định bắt buộc trong knowledge đang theo vị trí
(`03-solution.md#2`), đổi thứ tự sẽ đổi ID.

### Mốc B — Thực thi một task theo hợp đồng

- Bổ sung workspace/execution boundary, quyền, budget và persistence đủ tin cậy.
- Agent đọc bộ input đã pin, chạy implementation, verification và correction.
- Có review, evidence gắn candidate, xử lý interruption và giới hạn retry.
- Tạo change request khi cần amendment; chặn tự sửa hợp đồng đã chốt.
- Xuất result và gắn bằng chứng triển khai vào knowledge.

Nghiệm thu bằng một task có giới hạn và trace runner/agent/skill/tool thực tế,
theo [IMP-006](./improvements.md#imp-006). Các lỗi P1 trên đường chạy được chọn
phải được đóng; test stub không thay thế kiểm tra native client hoặc LLM thật
khi kết luận cần bằng chứng ở các mức đó.

**Tiến độ (23/09/2026, `src/run/`, `src/cli/run.rs`; intake MOC-B TASK-001…007):**

| Hạng mục | Đã có |
|---|---|
| Ranh giới thực thi | Mỗi lần chạy một git worktree `.zforge/worktrees/<RUN>` trên branch `zforge/<task>/<run>` từ commit baseline của manifest (TASK-002). Agent, lệnh test và fingerprint đều nhận work dir tường minh; code v1 vẫn dùng cwd như cũ |
| Input đã pin | Hợp đồng chỉ đọc từ snapshot `.records/revisions/` mà manifest pin, có kiểm hash; sửa file đang làm việc không đổi prompt. Task có dependency bị từ chối (Mốc C). `Flow::Contract` = Imported → Coded → Verified (TASK-003) |
| Vòng thực thi | `zforge run <HANDOVER> --task <T> [--async]` chạy verifier loop v1, mỗi lần gọi agent là Claude headless trong worktree với `--max-budget-usd` bằng phần còn lại (TASK-004) |
| Evidence và trace | Mỗi lần verify ghi candidate = fingerprint của worktree; mỗi lần gọi agent để lại trace IMP-006 trong `<run>/trace.jsonl` |
| Budget | Tổng cho mọi lần chạy của một task trong một handover; chạm trần thì `blocked`, không gọi agent nữa. Lần gọi bị kill giữa chừng tính là đã tiêu hết phần được cấp |
| Interruption và retry | Ctrl-C ghi `cancelled` (exit 130). Worker chết → `failed (interrupted)`, và mọi process group nó đã ghi bị dừng theo. `run retry` tạo run mới, `retry_of`, dùng phần budget còn lại (TASK-005) |
| Vận hành | `run status/list/cancel/clean`, `progress.md`/`result.md` sinh từ nhật ký sự kiện — nguồn sự thật duy nhất về trạng thái (TASK-001) |
| Amendment | Agent ghi change request vào `changes/` thì run dừng `blocked: amendment: CHANGE-<RUN>`; request được kiểm theo 5 mục §8; hợp đồng đã pin không đổi (TASK-006) |
| Knowledge | Requirement thành `verified` kèm run và candidate, chỉ suy từ record của run (TASK-007) |

`zforge doctor` có thêm check `runs`: run mà worker đã chết nhưng chưa được ghi
nhận, và run đã kết thúc còn giữ worktree.

Chưa làm: **TASK-008 (benchmark với Claude thật)** — người dùng chọn bỏ, sẽ dùng
thật rồi đánh giá từ log và trace. Vì vậy Mốc B **chưa được nghiệm thu** theo
tiêu chí ở trên: mọi kiểm chứng hiện tại đều bằng stub phát lại stream thật, chưa
có lượt chạy nào với model thật. MCP cho intake/run đã có (chỉ chuẩn bị và quan sát), còn các task của MOC-B được làm theo revision đang review, chưa chốt.

### Mốc C — Feature nhiều task

- Kiểm tra và điều phối dependency; bind output thực tế của task trước.
- Tích hợp và kiểm chứng feature/phase trên cùng candidate.
- Amendment chỉ vô hiệu phần bị ảnh hưởng; tiếp tục công việc độc lập khi phù hợp.
- Kiểm chứng hồ sơ nhiều phase mà người dùng vẫn review theo từng mức.

Chạy tuần tự có thể đáp ứng milestone ban đầu. Parallel execution, model routing
thích nghi và UI chuyên dụng chỉ bổ sung khi có nhu cầu và bằng chứng lợi ích.

**Tiến độ (23/09/2026, intake `MOC-C`, 8 task, đang review, chưa chốt):**

| Hạng mục | Đã có |
|---|---|
| Output của task | Test pass thì zforge niêm phong worktree thành commit trên branch của run (`run/output.rs`), chỉ nhận khi worktree sạch và fingerprint vẫn bằng candidate đã test; commit ghi trong event `verified` (TASK-001). Hook của repo luôn chạy; hook từ chối thì run `failed` |

| Điểm xuất phát | Task có dependency chạy được bằng `--task`: worktree tạo tại output của dependency, nhiều dependency thì merge theo thứ tự task liệt kê; `run.yaml` ghi `start` (commit + nguồn). Dependency chưa có output thì từ chối, không tạo gì; merge xung đột thì run `failed` nêu task và file, không gọi agent (TASK-002) |
| Kiểm chứng tích hợp | `zforge run <HANDOVER> --integration`: run `kind: integration` khi mọi task đã có output; worktree chứa output các task lá; chạy lệnh trong khối code đầu tiên của mục "Kiểm chứng tích hợp" đã pin (không có thì `project.test_command`), lệnh ghi trong `run.yaml`, output ở `checks.log`; lệnh fail đầu tiên làm run `failed`; không gọi agent, không tốn budget (TASK-003) |
| Trạng thái feature | `zforge run status <HANDOVER>`: mỗi task `ready`/`waiting`/`running`/`verified`/`failed`…/`blocked by`, và bước tích hợp — là hàm thuần của record các run, không có file trạng thái. Run verified mới nhất thắng run fail sau nó; task dừng chặn mọi task phía sau; việc làm trên output cũ của dependency được đánh dấu `outdated` (TASK-004) |
| Vòng chạy cả handover | `zforge run <HANDOVER> [--async]`: task theo thứ tự rồi tích hợp; chạy lại thì tiếp tục (task verified bỏ qua, run bị ngắt/hủy được chạy lại, `failed`/`blocked` giữ nguyên); khóa theo handover và theo task nên không có hai run của một task cùng lúc; `run cancel|log|wait <HANDOVER>`; MCP `run_start` không kèm task. Hạn chế: hủy lúc agent đang làm tiêu hết budget còn lại của task (quy tắc Mốc B), nên task đó cần handover mới (TASK-005) |

Còn lại: tái dùng, knowledge ba mức, nghiệm thu.

## 13. Kịch bản nghiệm thu workflow

| Kịch bản | Kết quả bắt buộc |
|---|---|
| Agent đánh dấu file là accepted nhưng không có quyết định người dùng | Không được bàn giao |
| File được sửa sau khi người dùng chốt | Không dùng bản sửa như revision đã được duyệt |
| Requirement bắt buộc chưa có criterion/task | Readiness thất bại dù mọi criterion hiện có đều pass |
| Task đã chốt nhưng dependency chưa có output hợp lệ | Chờ dependency, không hỏi lại nội dung đã chốt |
| Implementation gặp bug thông thường | Tự sửa trong hợp đồng, không tạo approval cho từng lần sửa |
| Agent cần đổi output hoặc quyết định bắt buộc | Tạo change request và dừng phần bị ảnh hưởng |
| Agent sửa test để bỏ một yêu cầu | Không được xác nhận output |
| Test không chạy được, run crash hoặc hết budget | Không được báo thành công; giữ hồ sơ để giải quyết/khôi phục |
| Tất cả task con pass nhưng tích hợp fail | Feature chưa verified; tự xử lý trong hợp đồng |
| Task verified ở branch riêng | Knowledge hiện đúng branch/tree, chưa nhận là đã tích hợp |
| Rule đã chốt nhưng chưa implement | Intake sau thấy đó là ý định, không mặc định là hành vi hiện tại |
| Người dùng chấp nhận amendment | Tạo revision và kiểm tra lại chính xác phần liên quan |
| Run tạm dừng rồi tiếp tục | Khôi phục hợp đồng, dependency, quyền và evidence đúng phiên bản |
| Init chọn client và task không override runner | Chạy đúng default đã lưu của project |
| Skill/tool bắt buộc thiếu hoặc client không nhận | Readiness thất bại trước dispatch, chỉ rõ capability thiếu |
| Verify mới fail sau một lần pass | Vô hiệu evidence pass cũ, chặn review/bàn giao |
| Timeout/cancel khi process con còn giữ pipe | Dừng cây process và trả kết quả trong giới hạn đã định |
| Refresh agent/profile sau khi đổi model | Giữ config người dùng, áp dụng model mới vào output được sinh |

Đánh giá trải nghiệm bằng việc người dùng có thể giải thích mục tiêu, luồng chính,
quyết định quan trọng và liên hệ tới task/code. Theo dõi câu hỏi lặp lại, quyết
định bị bỏ sót trong intake, amendment do intake thiếu căn cứ, sửa lại do hiểu
sai, và output được chấp nhận sau tự sửa. Không cần biến review thành bài thi
hoặc yêu cầu đo thời gian hoạt động của người dùng.

## 14. Các quyết định triển khai còn mở

- Định dạng metadata/schema, lưu revision và ghi quyết định có thẩm quyền.
- Cơ chế nhập thay đổi Markdown và bảo đảm bản được review khớp contract thực thi.
- Khôi phục run, ghi evidence và ranh giới cách ly cho phiên bản đầu.
- CLI/MCP tối thiểu và cách người dùng review/chốt qua editor hoặc chat.
- Mức tái sử dụng code v1 và quy tắc tương thích, nếu có.
- Cách phát hiện semantic drift, hiệu lực của knowledge và cập nhật reference.

Đề xuất cho từng mục, kèm điểm cần người dùng chọn: [decisions.md](./decisions.md).

Các mục này cần được chốt trong intake triển khai v1.5. Tên v1.5 không ngụ ý
phải xây toàn bộ nền tảng v2 hoặc kế thừa tự động các ADR của hướng đó.

Xem [tổng quan v1.5](./README.md). Tài liệu này không sửa flow v1, v2 hoặc runtime.
