# v1.5 — Đề xuất cho các quyết định triển khai còn mở

**Trạng thái: đề xuất chờ người dùng chốt. Chưa quyết định nào có hiệu lực.**

Tài liệu trả lời sáu điểm ở [§14 của workflow](./workflow.md#14-các-quyết-định-triển-khai-còn-mở).
Mỗi mục gồm bối cảnh, phương án, đề xuất, hệ quả và **điểm cần người dùng chọn**.
Đề xuất dựa trên runtime đã có sau Mốc 0: ghi atomic (`state::write_atomic`), task
lock, evidence gắn candidate (`src/evidence/`), trace từng lần chạy agent
(`src/trace/`), job nền với cancel/reconcile (`src/job/`), process có giới hạn
(`src/process.rs`). Mục tiêu đã thống nhất: team nhỏ, chất lượng tối đa, phụ thuộc
người tối thiểu; Claude Code là client chính.

Tóm tắt:

| # | Quyết định | Đề xuất |
|---|---|---|
| D1 | Metadata, revision, quyết định có thẩm quyền | Markdown là nội dung; runtime giữ snapshot + hash + nhật ký quyết định append-only; chỉ con người qua TTY mới chốt được |
| D2 | Nhập thay đổi Markdown, khớp bản review với contract | Chốt theo hash của bản đã review; thực thi đọc snapshot đã chốt, không đọc file đang sửa |
| D3 | Khôi phục run, evidence, cách ly | Một git worktree cho mỗi run; nhật ký sự kiện là nguồn sự thật; run bị ngắt → `failed(interrupted)`, chạy lại là run mới |
| D4 | CLI/MCP tối thiểu, review qua editor/chat | Cả hai: sửa trong editor, trao đổi qua chat; MCP không có lệnh chốt |
| D5 | Tái sử dụng v1, tương thích | Dùng lại toàn bộ hạ tầng; module `intake/` và `run/` mới; lệnh v1 giữ nguyên |
| D6 | Semantic drift, hiệu lực knowledge | Drift cơ học qua reference pin + git ancestry; drift ngữ nghĩa là đề xuất của agent, người dùng review |

---

## D1 — Metadata/schema, lưu revision và ghi quyết định có thẩm quyền

**Bối cảnh.** §2 yêu cầu: agent không được tự tạo xác nhận của người dùng; một dòng
`approved: true` do agent viết không có giá trị. §6.1 cần trạng thái tài liệu
(`draft → in_review → accepted → superseded`) tách khỏi trạng thái công việc.
Hiện tại v1 có MCP tool `approve` mà chính agent gọi được — đúng với v1, sai với v1.5.

**Phương án lưu trữ.**

| Phương án | Ưu | Nhược |
|---|---|---|
| A. Trạng thái trong frontmatter của file | Đơn giản | Agent sửa được → vi phạm §2; lẫn nội dung với trạng thái |
| B. File Markdown + record của runtime (snapshot, hash, nhật ký JSONL) | Nội dung dễ đọc; trạng thái chỉ runtime ghi; cùng kiểu với `verify-history.jsonl`, `trace.jsonl` đã có | Hai nơi cần đồng bộ; cần lệnh để xem |
| C. SQLite | Truy vấn mạnh, transaction | Thêm dependency, khó review bằng mắt/git, lệch khỏi phong cách file hiện tại |
| D. Git commit làm revision | Có sẵn diff/lịch sử | Buộc commit mỗi lần review; trộn lịch sử sản phẩm với lịch sử intake |

**Đề xuất: B.**

```text
.zforge/intakes/FEATURE-001/
├── 01-outcome.md …                 # nội dung, ai cũng sửa được (bản nháp)
└── .records/                        # chỉ runtime ghi
    ├── revisions/01-outcome/3.md    # snapshot đúng byte đã review
    └── decisions.jsonl              # append-only
```

Một dòng quyết định:

```json
{"at":"2026-10-01T09:12:00Z","file":"01-outcome.md","revision":3,
 "sha256":"…","decision":"accepted","by":"user","channel":"cli-tty","note":""}
```

- Hash: SHA-256 trên nội dung đã chuẩn hóa xuống dòng LF.
- Trạng thái tài liệu được **suy ra** từ nhật ký + hash của file hiện tại, không lưu
  riêng: nhật ký nói "rev 3 accepted", file hiện tại có hash khác → có bản nháp mới.
- Frontmatter chỉ chứa định danh cần cho máy (ID, parent, requirement IDs,
  depends_on), là một phần nội dung được hash và review. **Không có trường trạng
  thái hay phê duyệt nào trong file.**
- ID (`REQ-001`, `AC-01`, `TASK-002`) do agent đề xuất trong file; runtime kiểm tra
  duy nhất và không tái dùng ID của revision cũ.

**Thẩm quyền.** Chỉ `zforge intake accept` chạy với **stdin là TTY** và người dùng
gõ xác nhận (hiện hash rút gọn) mới ghi được `accepted`. Agent chạy trong
`claude -p` hay qua Bash tool không có TTY nên không làm được. Thêm vào
`.claude/settings.json` một luật `deny` cho `Bash(zforge intake accept*)` làm lớp
thứ hai. MCP không có tool chốt (xem D4).

> **Cần chọn (D1):** chốt chỉ qua CLI có TTY (đề xuất) — hay chấp nhận thêm kênh
> chat nếu người dùng gõ lại mã xác nhận hiện trên màn hình? Kênh thứ hai tiện hơn
> nhưng agent có thể đọc và gõ lại mã, nên không chứng minh được con người đã chốt.

---

## D2 — Nhập thay đổi Markdown và bảo đảm bản review khớp contract thực thi

**Bối cảnh.** §10: file được sửa trực tiếp khi intake; trước khi chốt, runtime phải
nhận đúng nội dung được review. §8: run dùng hợp đồng cũ không được đọc file mới như
thể nó là hợp đồng từ đầu.

**Đề xuất.**

1. **Sửa tự do** — agent và người dùng sửa file trong editor. Không có lệnh "import".
2. **Gửi review** — `zforge intake review <F> <file>`: chụp snapshot, tính hash, in
   diff so với revision đã chốt gần nhất, ghi `in_review` kèm hash vào nhật ký.
3. **Chốt** — `accept` chỉ thành công khi hash hiện tại **bằng** hash lúc gửi review.
   File bị sửa sau khi review → từ chối: "đã thay đổi từ lúc review, review lại".
   Chặn trường hợp agent sửa lén giữa lúc người dùng đọc và lúc chốt.
4. **Thực thi đọc snapshot** — manifest bàn giao (§6.3) ghi `(file, revision,
   sha256)`; prompt của run lấy nội dung từ `.records/revisions/…`, không từ file
   đang sửa. Bản nháp mới không ảnh hưởng run đang chạy.
5. **Kiểm tra cấu trúc** — một linter cho quy ước Markdown: section bắt buộc của
   leaf task (§5.6), cú pháp ID, reference tới ID tồn tại trong revision đã chốt.
   Linter chạy trong `review` và `readiness`; không thay thế review nội dung.

**Định dạng leaf task.** Markdown với các section cố định (như ví dụ §11) + frontmatter
nhỏ cho `id`, `parent`, `requirements`, `depends_on`. Không dùng YAML contract riêng
vì người dùng phải review đúng thứ agent thực thi, và thứ đó nên là văn bản đọc được.

**Diff.** Dùng `git diff --no-index` khi có git (đã là điều kiện của D3), không thêm
crate diff.

> **Cần chọn (D2):** leaf task là Markdown + frontmatter nhỏ (đề xuất), hay YAML
> contract có Markdown sinh ra để đọc? YAML dễ kiểm bằng máy hơn nhưng tạo hai bản
> mà người dùng phải tin là khớp nhau.

---

## D3 — Khôi phục run, ghi evidence và ranh giới cách ly cho phiên bản đầu

**Bối cảnh.** §6.1 định nghĩa trạng thái công việc; `failed`/`cancelled` kết thúc run,
chạy lại là run mới có lịch sử. IMP-002 còn thiếu recovery khi nhiều artifact cập nhật
dở dang. Runtime hiện có sẵn: evidence gắn candidate, trace, job + cancel, lock.

**Cách ly — phương án.**

| Phương án | Ưu | Nhược |
|---|---|---|
| A. Chạy trên working tree hiện tại (như v1) | Không cần gì thêm | Agent sửa cùng chỗ người dùng làm việc; không bỏ được một run hỏng; task song song không thể |
| B. Một git worktree cho mỗi run, branch `zforge/<task>/<run>` | Bỏ run hỏng dễ; candidate là tree của worktree; tích hợp là merge có kiểm soát | Bắt buộc git; cần dọn worktree |
| C. Container | Cách ly mạnh nhất | Nặng, phụ thuộc Docker, MCP/hook phải chạy trong container |

**Đề xuất: B**, git là điều kiện readiness (thiếu git → không bàn giao được). C để sau.

**Record của run.**

```text
.zforge/runs/RUN-007/
├── run.yaml        # task, manifest sha256, worktree, budget — ghi một lần lúc tạo
├── events.jsonl    # append-only: attempt, verify(candidate), trace ref, trạng thái
├── progress.md     # view sinh từ events, không có thẩm quyền
└── result.md       # view sinh từ events, không có thẩm quyền
```

- **Nguồn sự thật là `events.jsonl`.** Trạng thái run = phát lại sự kiện. Mỗi bước
  nhiều file (verify.md, lịch sử, state) kết thúc bằng một dòng sự kiện; dòng đó là
  điểm commit. Crash trước dòng đó → bước coi như chưa xảy ra và làm lại; view được
  sinh lại. Đây là cách đóng khoảng trống "transaction nhiều file" của IMP-002 mà
  không cần database.
- **Evidence** dùng lại `evidence::fingerprint` trên worktree của run; sự kiện
  `verified` chỉ hợp lệ khi candidate trùng tree tại thời điểm bàn giao.
- **Ngắt giữa chừng.** Khi đọc trạng thái, run `running` mà worker đã chết → ghi
  `failed` với lý do `interrupted` (cùng cơ chế `reconcile_dead_worker` đã có).
  Worktree được giữ. `zforge run retry <RUN>` tạo run mới pin cùng manifest; có thể
  lấy worktree cũ làm điểm xuất phát nhưng phải ghi rõ điều đó vào `run.yaml`.
- **Budget.** Mỗi spawn có `--max-budget-usd`; runtime cộng `cost_usd` từ trace và
  dừng run (`blocked: budget`) khi chạm giới hạn của manifest. Không tự nâng budget.

> **Cần chọn (D3):** bắt buộc git + worktree từ bản đầu (đề xuất), hay cho phép
> chạy trên working tree hiện tại ở MVP và thêm worktree sau? Bản không có worktree
> nhanh hơn để có Mốc B nhưng không bỏ được run hỏng và không chạy song song được ở
> Mốc C.

---

## D4 — CLI/MCP tối thiểu; review/chốt qua editor hay chat

**Bối cảnh.** Người dùng làm việc trong Claude Code (chat) và editor. Agent intake cần
tool để đọc/ghi; người dùng cần cách chốt mà agent không giả mạo được (D1).

**Đề xuất: cả hai, chia vai.** Nội dung được sửa trong editor. Trao đổi, giải thích và
điều chỉnh diễn ra trong chat với agent intake. **Chốt luôn ở terminal** (D1).

CLI tối thiểu cho Mốc A–B:

| Lệnh | Việc |
|---|---|
| `zforge intake new <F> [--from <task\|jira>]` | Tạo thư mục intake, file stage rỗng theo template |
| `zforge intake status <F> [--json]` | Stage, trạng thái tài liệu, câu hỏi còn mở, readiness tóm tắt |
| `zforge intake review <F> <file>` | Snapshot + diff, ghi `in_review` |
| `zforge intake accept <F> <file>` | Chỉ TTY: ghi `accepted` cho đúng hash đã review |
| `zforge intake revise <F> <file> --note …` | Ghi `needs_revision` kèm lý do |
| `zforge readiness <F> [--scope <phase>]` | Kiểm tra §6.2, ghi `readiness.md` |
| `zforge handover <F> [--scope …]` | Chỉ TTY: tạo manifest từ các revision đã chốt |
| `zforge run <TASK>` · `run status` · `run cancel` · `run retry` | Thực thi theo manifest (dùng lại job) |
| `zforge change new <F>` · `change accept <CHANGE>` | Change request §8; `accept` chỉ TTY |

MCP cho agent: `intake_status`, `intake_diff`, `readiness`, `run_status`,
`request_review` (đưa file vào hàng chờ review và báo người dùng), `change_new`.
**Không có** `accept`, `handover`, `change_accept` trên MCP. Tool `approve` của v1 giữ
nguyên cho flow v1, không dùng được cho intake.

> **Cần chọn (D4):** thứ tự làm — CLI trước, MCP sau (đề xuất, vì chốt vốn ở CLI),
> hay làm song song để agent intake dùng được ngay trong chat?

---

## D5 — Mức tái sử dụng code v1 và quy tắc tương thích

**Bối cảnh.** Mốc 0 đã gia cố hạ tầng v1 và có regression test. FSM của v1
(`Imported → … → Reviewed`) mô tả một pipeline khác với intake của v1.5.

**Đề xuất.**

- **Dùng lại nguyên vẹn:** `process`, `state::write_atomic`, cơ chế task lock,
  `evidence`, `trace`, `orchestrator` (spawn, fallback, model routing, named agent,
  headless), `job` (worker, cancel, reconcile), `registry`, `cost`, `doctor`,
  `init` + native skills.
- **Viết mới:** `src/intake/` (record, hash, linter, readiness, manifest) và
  `src/run/` (worktree, events, budget), cùng lệnh CLI/MCP ở D4.
- **Không dùng lại FSM v1 cho tài liệu intake.** Với leaf task, thêm một `Flow`
  mới `Contract` (code → verify) để tận dụng `ship` và vòng verifier: hợp đồng đã
  chốt thay cho spec/testspec/plan làm context của prompt code.
- **Tương thích:** mọi lệnh v1 và `.zforge/tasks/` giữ nguyên hành vi; artifact v1.5
  nằm trong `.zforge/intakes/`, `.zforge/runs/`, `.zforge/knowledge/`. Không migrate
  task v1. Khi intake được phát hành, tăng package lên `0.2.0`.
- ADR của v2 không tự áp dụng (theo README v1.5); chỉ lấy lại ý tưởng khi có lý do
  ghi trong tài liệu này.

> **Cần chọn (D5):** leaf task chạy qua `Flow::Contract` trên pipeline v1 (đề xuất,
> tái dùng nhiều nhất), hay một vòng thực thi riêng trong `run/` gọi thẳng
> orchestrator? Vòng riêng gọn hơn về khái niệm nhưng phải làm lại phần đã có test.

---

## D6 — Phát hiện semantic drift, hiệu lực knowledge và cập nhật reference

**Bối cảnh.** §9: knowledge tách *quyết định* khỏi *triển khai*; không suy ra việc tích
hợp từ lời agent; tổng hợp làm đổi ý nghĩa phải qua review.

**Đề xuất: phân hai loại drift.**

1. **Drift cơ học — runtime phát hiện, không cần model.**
   - Mọi reference pin `(file, revision, sha256)`. Revision được tham chiếu đã bị
     `superseded` → reference cũ; `readiness` báo và chặn bàn giao phần phụ thuộc.
   - Trạng thái triển khai trong `knowledge/index.md` chỉ lấy từ record: `verified`
     từ sự kiện run có candidate; `integrated` khi
     `git merge-base --is-ancestor <commit> <baseline>` trả đúng. Không bao giờ từ
     câu trả lời của agent.
   - Code ở baseline đổi trên các file mà task đã verify → đánh dấu evidence của
     knowledge đó là "cần kiểm lại" (cùng cơ chế evidence hiện tại).
2. **Drift ngữ nghĩa — agent đề xuất, người dùng quyết.** Khi mở intake mới, agent
   đọc knowledge còn hiệu lực và liệt kê trong `01-outcome.md` các điểm yêu cầu mới
   mâu thuẫn hoặc mở rộng quyết định cũ. Danh sách này được review và chốt như mọi
   nội dung khác; không tự sửa knowledge.

**Index.** `knowledge/index.md` sinh ra từ record, mỗi mục là liên kết tới nội dung đã
chốt kèm hai cột trạng thái (quyết định / triển khai). Không sao chép hay diễn giải
lại nội dung (§9.2). Embedding hay tìm kiếm ngữ nghĩa để sau, khi có nhu cầu thật.

> **Cần chọn (D6):** "integrated" so với nhánh nào — một baseline cố định trong
> config (ví dụ `main`), hay baseline ghi riêng trong từng manifest?

---

## Thứ tự nếu các đề xuất được chấp nhận

1. `intake/` record + hash + `intake new|status|review|accept|revise` (D1, D2, D4).
2. Linter + `readiness` + `handover` → manifest (D2, D4).
3. `run/` với worktree + events + `Flow::Contract`; nghiệm thu bằng benchmark
   IMP-006 trên một leaf task (D3, D5) — đây là Mốc B.
4. `knowledge/index.md` + drift cơ học (D6); drift ngữ nghĩa trong prompt intake.

Mốc A có thể bắt đầu ngay sau khi D1, D2, D4 được chốt; D3, D5 cần trước Mốc B; D6
trước khi dùng knowledge cho intake thứ hai.
