# v1.5 — Dùng intake và run

Hướng dẫn đi hết một vòng: từ yêu cầu tới code đã kiểm chứng trên branch riêng.
Phần thiết kế ở [workflow](./workflow.md), các quyết định ở
[decisions](./decisions.md).

**Trạng thái:** Mốc A và Mốc B đã dùng được. Chưa có lượt chạy nào với model thật
(TASK-008 bị bỏ), nên hãy coi những lần chạy đầu là vừa dùng vừa kiểm chứng.

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
kiểm chứng tích hợp, có git và có budget. Ghi `readiness.md` để đọc.

```bash
zforge handover FEATURE-001
```

Cũng chỉ chạy ở terminal thật. Tạo `HANDOVER-001` pin từng file kèm revision và
hash, thứ tự task, baseline branch + commit, budget và số vòng verify.

Bàn giao một phần:

```bash
zforge handover FEATURE-001 --task TASK-001 --task TASK-002
```

## 3. Chạy một task

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

Kết thúc thế nào:

| Trạng thái | Nghĩa |
|---|---|
| `verified` | Test pass trên tree của worktree; candidate được ghi lại |
| `failed` | Hết số vòng verify, agent lỗi/timeout, hoặc không khởi động được |
| `blocked` | Hết budget, hoặc agent xin sửa hợp đồng (`amendment: CHANGE-RUN-001`) |
| `cancelled` | Bạn hủy, hoặc Ctrl-C |

Budget là **tổng cho mọi lần chạy của một task trong một handover**. Hết thì phải
handover mới, tức bạn quyết định chi thêm. Lần gọi agent bị kill giữa chừng bị
tính là đã tiêu hết phần được cấp, vì không biết nó đã tiêu bao nhiêu.

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
`superseded`) và trạng thái triển khai (`not_implemented`, `handed_over`,
`verified (RUN-001, candidate …)`).

## Khi agent xin sửa hợp đồng

Run dừng ở `blocked` và để lại
`.zforge/intakes/FEATURE-001/changes/CHANGE-RUN-001.md`. Đọc file đó, rồi:

- **Đồng ý:** sửa file hợp đồng → `intake review` → `intake accept` →
  `zforge handover` mới → `zforge run` trên handover mới.
- **Không đồng ý:** để run ở `blocked`, hoặc `run retry` nếu bạn cho rằng vẫn
  làm được trong hợp đồng cũ.

Không có lệnh "accept change request". Sửa hợp đồng luôn đi qua review và accept.

## Giới hạn hiện tại

- Task có `depends_on` **chưa chạy được**; ghép output giữa các task thuộc Mốc C.
- Chỉ Claude chạy được leaf task.
- Run chạy với `--dangerously-skip-permissions` để không cần người trả lời quyền.
  Agent bị giữ trong worktree bằng cwd, nhưng quyền trên máy vẫn là quyền của bạn.
- Chưa có MCP cho intake/run; agent dùng qua Bash.
- `zforge doctor` chưa kiểm phần Mốc B (worktree, run còn treo).

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
