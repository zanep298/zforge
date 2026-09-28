# v1.5 — Dùng intake và run

Hướng dẫn đi hết một vòng: từ yêu cầu tới code đã kiểm chứng trên branch riêng.
Phần thiết kế ở [workflow](./workflow.md), các quyết định ở
[decisions](./decisions.md).

**Trạng thái:** Mốc A, B và C đã dùng được: intake, chạy một task, và chạy cả
feature nhiều task có dependency, kiểm chứng tích hợp, tái dùng qua handover.
Mọi kiểm chứng đều bằng stub; chưa có lượt chạy nào với model thật, nên hãy coi
những lần chạy đầu là vừa dùng vừa kiểm chứng.

## Chuẩn bị một lần

```bash
cargo install --path . --force
```

`zforge` trên PATH phải là bản này. Nếu `which -a zforge` cho ra
`/opt/homebrew/bin/zforge` trước `~/.cargo/bin/zforge`, sửa PATH hoặc gỡ bản
Homebrew — bản cũ không có lệnh `intake` và sẽ báo `unrecognized subcommand`.

Project cần:

- là git repo, có ít nhất một commit (mỗi lần chạy tạo worktree từ một commit);
- `.zforge/config.yaml` với `project.test_command` chạy được;
- `execution.budget_usd` (readiness bắt buộc) và `execution.max_iterations`;
- `claude` trong registry (`zforge init --agent claude` làm việc này).

```yaml
# .zforge/config.yaml
project:
  name: "my-app"
  language: "rust"
  test_command: "cargo test"
knowledge:
  baseline: "main"
execution:
  max_iterations: 3
  budget_usd: 3.0
```

## 1. Intake: làm rõ và chốt

Agent chuẩn bị nội dung (skill `zforge-intake`), bạn quyết định.

```bash
zforge intake new FEATURE-001
```

Điền `01-outcome.md` → `04-breakdown.md`, rồi mỗi leaf task:

```bash
zforge intake task FEATURE-001 TASK-001
```

Gửi review và xem trạng thái:

```bash
zforge intake review FEATURE-001 01-outcome.md
```
```bash
zforge intake status FEATURE-001
```

Chốt (**chỉ chạy được ở terminal thật, và phải gõ xác nhận**):

```bash
zforge intake accept FEATURE-001 01-outcome.md
```

Cần sửa thì:

```bash
zforge intake revise FEATURE-001 01-outcome.md --note "nêu rõ trạng thái archived"
```

Quy tắc đáng nhớ:

- Sửa file sau khi gửi review thì không chốt được nữa; phải review lại. Đây là
  cách chặn việc nội dung bạn đọc khác nội dung được chốt.
- Câu hỏi mở viết dạng `- [ ] …` dưới mục "Câu hỏi còn mở". Còn dấu `[ ]` thì
  readiness không cho bàn giao.
- Trạng thái không nằm trong file. Nó suy ra từ `.records/decisions.jsonl`.

## 2. Readiness và bàn giao

```bash
zforge readiness FEATURE-001
```

Kiểm trên **bản đã chốt**: mọi file đã accept và không còn bản nháp, cấu trúc và
ID hợp lệ, không còn câu hỏi mở, mọi REQ có task, dependency không vòng, có mục
kiểm chứng tích hợp, có git, có budget, có `test_command`, runner `claude` có
trong registry và cài trên máy, và **không file nào cũ so với file nó dựa vào**.
Ghi `readiness.md` để đọc.

File cũ: một file được chốt trước khi file phía trên nó (stage trước, hoặc task
mà nó phụ thuộc) có revision chốt mới hơn. Có thể nó vẫn đúng, nhưng bạn phải
xác nhận: `zforge intake review` rồi `zforge intake accept` lại file đó, kể cả
khi không sửa gì (revision tăng, hash giữ nguyên). Xác nhận lại `04-breakdown.md`
làm các task thành cũ theo, nên đi từ trên xuống.

```bash
zforge handover FEATURE-001
```

Cũng chỉ chạy ở terminal thật. Tạo `HANDOVER-001` pin từng file kèm revision và
hash, thứ tự task, baseline branch + commit, budget và số vòng verify.

Bàn giao một phần:

```bash
zforge handover FEATURE-001 --task TASK-001 --task TASK-002
```

## 3. Chạy

Cả handover — mọi task theo thứ tự dependency, rồi kiểm chứng tích hợp:

```bash
zforge run HANDOVER-001
```

Task có dependency bắt đầu từ output đã verified của dependency. Task dừng
(`failed`, `blocked`) chặn các task phía sau nó; task không phụ thuộc vẫn chạy
tiếp. Bị ngắt thì chạy lại đúng lệnh đó: task đã verified được bỏ qua, run bị
ngắt hoặc bị hủy được chạy lại (`retry_of`); run `failed`/`blocked` là kết luận,
giữ nguyên cho bạn quyết. Chạy nền thì thêm `--async`, rồi
`zforge run log|wait|cancel HANDOVER-001`.

Sau amendment và handover mới, `zforge run HANDOVER-002` **tái dùng** output
của task không đổi từ handover trước — chỉ khi hash của 4 stage, file task, mọi
dependency của nó và commit baseline đều trùng, và nó được làm trên đúng output
mà dependency đang có. Task được tái dùng hiện `reused`, không gọi agent. Muốn
chạy lại một task dù không đổi thì dùng `--task`.

Một task riêng:

```bash
zforge run HANDOVER-001 --task TASK-001
```

Chuyện xảy ra: kiểm hash hợp đồng → tạo worktree `.zforge/worktrees/RUN-001`
trên branch `zforge/TASK-001/RUN-001` từ commit baseline → vòng code → verify →
sửa theo phản hồi, tối đa `max_iterations` lần verify. Agent là Claude chạy
headless **trong worktree**; working tree của bạn không bị đụng.

Chạy nền và theo dõi:

```bash
zforge run HANDOVER-001 --task TASK-001 --async
```
```bash
zforge run log RUN-001 --follow
```
```bash
zforge run status RUN-001
```

Các lệnh khác: `zforge run list`, `run wait`, `run cancel`, `run retry`,
`run clean`.

Khi mọi task của handover đã `verified`, kiểm chứng cả feature trên một tree
chứa output của tất cả:

```bash
zforge run HANDOVER-001 --integration
```

Xem cả handover đang ở đâu — từng task và bước tích hợp:

```bash
zforge run status HANDOVER-001
```

Lệnh tích hợp lấy từ khối code đầu tiên dưới mục "Kiểm chứng tích hợp" của
`04-breakdown.md` đã chốt, mỗi dòng một lệnh; không có khối code thì dùng
`project.test_command`. Không gọi agent: tích hợp fail là chuyện hợp đồng giữa
các task, sửa qua amendment.

Agent trong chat làm được phần chuẩn bị và theo dõi qua MCP: `intake_new`,
`intake_task`, `intake_status`, `intake_review`, `intake_diff`, `change_new`,
`readiness`, `knowledge_index`, `run_start`, `run_status`, `run_list`,
`run_log`, `run_cancel`. `run_start` không kèm `task` chạy cả handover trong
nền; `run_status`, `run_log`, `run_cancel` nhận `HANDOVER-001` như một run.
**Không** có tool nào chốt hay handover được.

Kết thúc thế nào:

| Trạng thái | Nghĩa |
|---|---|
| `verified` | Test pass trên tree của worktree; candidate được ghi lại |
| `failed` | Hết số vòng verify, agent lỗi/timeout, hoặc không khởi động được |
| `blocked` | Hết budget, hoặc agent xin sửa hợp đồng (`amendment: CHANGE-RUN-001`) |
| `cancelled` | Bạn hủy, hoặc Ctrl-C |

**Agent nhận gì ngoài hợp đồng.** Worktree chỉ có những gì đã commit, nên run
tự bổ sung: prompt liệt kê các checklist viết code và test (`write-tests-first`,
`implement-minimal-patch`, `<ngôn ngữ>-patterns`/`-testing`) bằng đường dẫn tuyệt
đối trong store skill; và nếu project **ignore** `.claude/`, run chép
`.claude/agents` và `.claude/skills` từ checkout chính vào worktree để
`code-agent` và skill nạp sẵn hoạt động. `.claude/` không commit mà cũng không
ignore thì run không chép (output sẽ mang theo nó) và cảnh báo: commit nó, hoặc
thêm vào `.gitignore`.

**Test là của hợp đồng, không phải của agent.** Khi test pass, zforge kiểm
các file test có sẵn so với lúc task bắt đầu: file nào bị sửa, xóa hay đổi tên
thì lần pass đó bị tính là fail (`protected test changed: <file>`), và agent
được báo để khôi phục. Thêm test mới thì được. File được bảo vệ: vị trí test
quen thuộc (`tests/**`, `test/**`, `*_test.*`, `test_*.*`, `*.test.*`,
`*.spec.*`, …) và file mà `test_command` gọi (`sh test.sh` → `test.sh`). Đổi
danh sách bằng `execution.protected_tests` trong config. Một task được phép sửa
test cụ thể chỉ khi hợp đồng của nó ghi, trong frontmatter:

```yaml
tests_may_change: [tests/api_test.rs]
```

Giới hạn: test nằm chung file với code (module test trong file Rust) hay cấu
hình test runner ở chỗ khác thì không phát hiện được.

Budget là **tổng cho mọi lần chạy của một task trong một handover**. Hết thì phải
handover mới, tức bạn quyết định chi thêm. Lần gọi agent bị kill giữa chừng bị
tính là đã tiêu hết phần được cấp, vì không biết nó đã tiêu bao nhiêu — và phần
được cấp là **toàn bộ budget còn lại của task**. Nên hủy hay Ctrl-C lúc agent
đang làm thì task đó không chạy lại được trong handover này (lệnh báo `budget …
is used up`); bị ngắt lúc đang verify thì chạy lại được, vì chi phí lần gọi
agent đã được chốt.

## 4. Xem kết quả và dọn

Agent commit vào branch của run. Xem thay đổi:

```bash
git log --stat zforge/TASK-001/RUN-001
```

Ưng thì merge như mọi branch khác. zforge không bao giờ push hay merge hộ.

```bash
zforge run clean RUN-001
```

Xóa worktree, giữ branch. Phần chưa commit trong worktree được commit vào branch
trước khi xóa, nên không mất gì.

```bash
zforge knowledge index
```

Sinh `.zforge/knowledge/index.md`: mỗi requirement và mỗi quyết định bắt buộc,
kèm nguồn (file, revision, hash), trạng thái quyết định (`active` hay
`superseded`) và trạng thái triển khai — mức cao nhất đúng: `not_implemented`,
`handed_over`, `verified (RUN-001, candidate …)` (task pass trên branch riêng),
`integration verified (RUN-005 of HANDOVER-001, …)` (cả feature pass kiểm chứng
tích hợp), `integrated (…, <commit> in main)` (output tích hợp đã nằm trong
branch baseline). `integrated` được hỏi lại git mỗi lần; merge kiểu squash
hoặc rebase không giữ commit nên vẫn là `integration verified`.

## Khi agent xin sửa hợp đồng

Run dừng ở `blocked` và để lại
`.zforge/intakes/FEATURE-001/changes/CHANGE-RUN-001.md`. Đọc file đó, rồi:

- **Đồng ý:** sửa file hợp đồng → `intake review` → `intake accept` →
  `zforge handover` mới → `zforge run` trên handover mới.
- **Không đồng ý:** để run ở `blocked`, hoặc `run retry` nếu bạn cho rằng vẫn
  làm được trong hợp đồng cũ.

Không có lệnh "accept change request". Sửa hợp đồng luôn đi qua review và accept.

## Giới hạn hiện tại

- Chạy tuần tự, một run một lúc.
- Chỉ Claude chạy được leaf task.
- Run chạy với `--dangerously-skip-permissions` để không cần người trả lời quyền.
  Agent bị giữ trong worktree bằng cwd, nhưng quyền trên máy vẫn là quyền của bạn.
- MCP chỉ có tool chuẩn bị và quan sát. Chốt, handover và `run clean` vẫn phải
  chạy ở terminal.

## Ghi lại để đánh giá sau

Mỗi lần chạy để lại, dưới `.zforge/runs/RUN-nnn/`:

| File | Nội dung |
|---|---|
| `run.yaml` | Hợp đồng nào, branch nào, baseline nào, budget bao nhiêu |
| `events.jsonl` | Nguồn sự thật: từng lần gọi agent kèm chi phí, từng lần verify kèm candidate, lý do dừng |
| `trace.jsonl` | Mỗi lần gọi agent: model, phiên bản client, MCP server, tool đã gọi, findings |
| `result.md`, `progress.md` | Bản đọc được, sinh lại từ hai file trên |
| `log` | stdout/stderr của worker (chỉ khi `--async`) |

Đây là dữ liệu để đánh giá chất lượng và chi phí về sau, không cần benchmark
riêng.
