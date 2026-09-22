# v1.5 — Backlog sửa lỗi runtime và init

**Trạng thái: FIX-001 → FIX-017 đã sửa, mỗi mục có regression test. Review nhánh sửa lỗi ngày 21–22/09/2026 tìm thêm FIX-018 → FIX-026, cũng đã sửa ([xem cuối tài liệu](#review-nhánh-sửa-lỗi-2122092026)).**

Tài liệu tập hợp 11 nhóm lỗi runtime và 6 nhóm lỗi init/dispatch từ hai lượt
review. Khoảng trống native skills và native custom agent được đưa vào
[kế hoạch cải tiến](./improvements.md#imp-004).

Baseline là package **0.1.8**, HEAD `38ddb5f`, bao gồm thay đổi chưa commit tại
thời điểm review. Kiểm tra runtime ngày 16/09/2026, báo cáo ngày 17/09/2026;
kiểm tra init/tool ngày 18/09/2026. HEAD riêng lẻ không tái tạo đầy đủ baseline
vì checkout có thay đổi chưa commit. Khi bắt đầu sửa, cần tái hiện trên checkout
thực tế và lưu regression test trong repository.

Các lỗi thuộc runtime hiện có, được đưa vào backlog v1.5 để bảo đảm nền tảng
thực thi đáng tin cậy. Việc ghi nhận không xác định quan hệ kế thừa runtime,
package version hay áp dụng tự động các ADR của v2.

## Bằng chứng và giới hạn

- `cargo test --all-targets --offline` báo 428 passed, 0 failed. Trong đó 13 test
  agent thật tự return vì chưa bật `ZFORGE_TEST_REAL_*`; kết quả này không chứng
  minh tương thích với CLI/agent thật.
- Review runtime dùng 15 tình huống bổ sung trong project tạm, shell stub và
  lệnh `true`/`false`. Các trường hợp chỉ suy luận từ source được ghi riêng.
- Review init dùng 6 cấu hình sạch: Claude, Codex, OpenCode × shared/local;
  capture 6 lần dispatch `spec`, thêm tình huống default runner và refresh model.
  Installer/LLM runner dùng stub, với home/config riêng cho subprocess.
- CodeGraph 0.9.4 thật: init, index, MCP initialize, tools/list và search thành
  công, tìm đúng function `hello` trong fixture JavaScript.
- OpenCode 1.14.41 thật: `--pure debug skill` trả `[]`; debug `spec-agent` nhận
  được definition và model. Đối chiếu help của Codex 0.145.0, Claude và RTK 0.37.2.
- Chưa chạy suy luận LLM thật để chứng minh agent đọc skill hoặc chọn đúng tool.
  Các kết quả trên là bằng chứng của đợt review, không phải lần chạy lại sau fix.

P1 cần sửa trước khi tin cậy execution tự động trên đường chạy bị ảnh hưởng.
P2 là lỗi chức năng/tính nhất quán cần sửa trước khi công bố hỗ trợ đầy đủ tính
năng tương ứng.

**Đã đóng:** tất cả 17 mục (xem ghi chú trạng thái tại từng mục). Các giới hạn
còn lại được ghi riêng trong từng mục.
Các mục còn lại đang **open**.

## Danh mục

| ID | Ưu tiên | Vấn đề | Bằng chứng |
|---|---|---|---|
| [FIX-001](#fix-001) | P1 | ~~Verify/ship/job báo thành công khi test fail~~ **đã sửa** | Regression test |
| [FIX-002](#fix-002) | P1 | ~~Verify lại giữ hiệu lực pass cũ hoặc lỗi transition~~ **đã sửa** | Regression test |
| [FIX-003](#fix-003) | P1 | ~~Log thường làm bẩn stdout MCP~~ **đã sửa** | Regression test |
| [FIX-004](#fix-004) | P1 | ~~Mutation khác vượt qua task lock~~ **đã sửa** | Regression test |
| [FIX-005](#fix-005) | P1 | ~~Retry vượt gate hoặc reset ra ngoài flow~~ **đã sửa** | Regression test |
| [FIX-006](#fix-006) | P1 | ~~Timeout không giới hạn được process con giữ pipe~~ **đã sửa** | Regression test |
| [FIX-007](#fix-007) | P2 | ~~Ship ghi đè lịch sử fallback~~ **đã sửa** | Regression test |
| [FIX-008](#fix-008) | P2 | ~~Resume ở Coded bỏ vòng tự sửa~~ **đã sửa** | Regression test |
| [FIX-009](#fix-009) | P2 | ~~Worker lỗi khởi động để job ở queued~~ **đã sửa** | Regression test |
| [FIX-010](#fix-010) | P2 | ~~Command chứa dấu nháy làm hỏng verify.md~~ **đã sửa** | Regression test |
| [FIX-011](#fix-011) | P2 | ~~Ship chạy code rồi mới lỗi ở flow không có verify~~ **đã sửa** | Regression test |
| [FIX-012](#fix-012) | P1 | ~~Thiếu đăng ký CodeGraph cho Codex/OpenCode~~ **đã sửa** | Regression test |
| [FIX-013](#fix-013) | P1 | ~~Shared mode trỏ sai đường dẫn skills~~ **đã sửa** | Regression test |
| [FIX-014](#fix-014) | P2 | ~~Dispatch không chọn named phase agent~~ **đã sửa** | Regression test |
| [FIX-015](#fix-015) | P1 | ~~Init Codex nhưng default runner vẫn là Claude~~ **đã sửa** | Regression test |
| [FIX-016](#fix-016) | P2 | ~~RTK setup không theo target client~~ **đã sửa** | Regression test |
| [FIX-017](#fix-017) | P2 | ~~Refresh bằng --force xóa model tùy chỉnh~~ **đã sửa** | Regression test |

## Runtime

### FIX-001

**[P1] Kết quả test thất bại phải truyền tới CLI và job.**

- Hiện trạng: task ở `Coded`, test command `/usr/bin/false`; `verify` exit 0 dù
  `verify.md` ghi `passed: false`. `ship --async` mặc định cũng kết thúc success.
- Vị trí: [verify](../../src/cli/verify.rs), [ship](../../src/cli/ship.rs),
  [worker](../../src/job/worker.rs). `verify::run` bỏ `VerifyOutcome.passed`.
- Sửa: giữ outcome có cấu trúc cho vòng tự sửa, ánh xạ thất bại sang nonzero tại
  CLI/worker. Các nhánh `BLOCKED` trong code/ship không được trả thành công.
- Nghiệm thu: test pass/fail, gate blocked và hết retry budget cho kết quả nhất
  quán giữa CLI, MCP và job. Job success chỉ khi các gate bắt buộc pass.
  Sửa assertion đang đóng băng hành vi sai trong
  [verifier_loop_test](../../tests/verifier_loop_test.rs).

**Trạng thái: đã sửa.** `OperationOutcome` (`src/cli/outcome.rs`) là kết quả có
cấu trúc dùng chung; `verify::run`, `ship::run` và `job::worker::run` trả outcome
thay vì `Ok(())`. CLI ánh xạ sang exit code (0 success, 1 failed, 2 blocked,
124 timeout, 130 cancelled), MCP đặt `isError`, worker ghi job failed. Nhánh
`BLOCKED` của ship trả `Blocked` thay vì success. Assertion đóng băng hành vi sai
trong `verifier_loop_test` đã được đảo lại.

Regression: `tests/cli_exit_code_test.rs` (exit code qua binary thật),
`tests/background_job_test.rs::async_single_shot_ship_marks_failed_when_tests_fail`,
`tests/verifier_loop_test.rs::{single_shot_ship_propagates_verify_failure,
ship_reports_blocked_when_gate_refuses}`.

Giới hạn còn lại: test timeout đã ra `Timeout` (FIX-006); timeout của agent và
`Cancelled` vẫn chưa có producer riêng.

### FIX-002

**[P1] Verification mới phải quyết định hiệu lực trạng thái và review.**

- Hiện trạng: pass lần hai lỗi `Verified → Verified`; fail sau một lần pass vẫn
  giữ `Verified`, rồi `review --done` vẫn đưa task lên `Reviewed`.
- Vị trí: [verify](../../src/cli/verify.rs), [review](../../src/cli/review.rs).
- Sửa: cho phép reverify hợp lệ; fail phải vô hiệu pass cũ và chặn review.
  Chốt quy tắc cho reverify task đã Reviewed, bảo toàn lịch sử từng lần chạy.
- Nghiệm thu: pass→pass không lỗi; pass→fail chặn review; fail→pass phục hồi
  đúng. Evidence phải thuộc candidate/input đang được xác nhận, không chỉ dựa
  trên tên trạng thái. Kiểm tra riêng trường hợp task đã Reviewed.

**Trạng thái: đã sửa (một phần nghiệm thu còn mở).** `verify` tách hai nhánh:
pass khi state ≥ Verified ghi `record_reverify` và giữ nguyên state (hết lỗi
`Verified → Verified`); fail gọi `TaskState::invalidate_to(Coded)` để rút lại
pass cũ. `invalidate_to` append history thay vì prune (khác `reset_to` của
retry), nên vẫn truy được task đã từng đạt Verified. Task đã `Reviewed` cũng bị
kéo về `Coded` khi reverify fail — review dựa trên pass đã bị rút cũng không còn
hiệu lực. `review --done` thêm `ensure_verify_evidence_passes` đọc lại
`verify.md`.

Regression: `tests/reverify_test.rs` (6 case: pass→pass, pass→fail, chặn review,
fail→pass, xuyên qua Reviewed, pass lại giữ Reviewed) + unit test
`invalidate_to` trong `src/state/mod.rs`.

Giới hạn này đã được đóng bởi [IMP-002](./improvements.md#imp-002): `review --done`
giờ đòi evidence gắn với đúng tree code hiện tại và đúng test command đã cấu hình
(ngoài git chỉ cảnh báo vì không lấy được fingerprint).

### FIX-003

**[P1] Stdout MCP chỉ chứa thông điệp protocol.**

- Hiện trạng: một request MCP `verify` sinh 5 dòng text thường trước JSON
  response. Tắt ANSI không giải quyết được; chưa đo phản ứng của từng client.
- Vị trí: [MCP dispatcher](../../src/mcp/mod.rs), [verify](../../src/cli/verify.rs).
- Sửa: tách operation khỏi CLI renderer; log server đi stderr, protocol đi stdout.
- Nghiệm thu: chạy subprocess MCP thật với request thành công và thất bại; mọi
  thông điệp stdout parse được theo transport JSON-RPC, response đúng request ID.
  Dùng cùng kiểm tra cho các tool gọi operation có in tiến trình.

**Trạng thái: đã sửa.** `src/cli/output.rs` có sink toàn cục + macro `note!`;
`mcp::run` gọi `divert_to_stderr()` một lần lúc khởi động. 166 call site
`println!` trong `src/cli/*` và `src/prompt/engine.rs` chuyển sang `note!`. CLI
không đổi hành vi (sink mặc định là stdout); dưới MCP mọi tiến trình đi stderr.

Regression: `tests/mcp_stdout_test.rs` chạy subprocess `zforge mcp` thật, khẳng
định mọi dòng stdout parse được thành JSON-RPC 2.0, ID response khớp request
theo thứ tự, và request thất bại vẫn trả frame hợp lệ. Đã xác nhận cả 4 test
này fail khi tắt `divert_to_stderr()`.

Giới hạn còn lại: `init.rs`, `install.rs`, `update.rs`, `mcp_register.rs` vẫn
dùng `println!` — không nằm trên đường MCP. Một `println!` mới thêm vào module
trên đường MCP sẽ không bị chặn lúc compile; `mcp_stdout_test` là lưới an toàn
duy nhất hiện nay.

### FIX-004

**[P1] Mọi mutation của cùng task phải tuân thủ khóa chung.**

- Hiện trạng: `ship` giữ lock và chạy test chậm; `retry --from spec --yes` vẫn
  xóa artifact/reset task. Ship sau đó lưu state cũ thành Verified.
- Vị trí: [retry](../../src/cli/retry.rs), [ship](../../src/cli/ship.rs),
  [task state](../../src/state/mod.rs).
- Sửa: bảo vệ toàn bộ chuỗi đọc-sửa-ghi state/artifact qua CLI và MCP. Truyền
  lock ownership rõ ràng khi gọi lồng nhau, không bỏ khóa chỉ vì biến môi trường.
- Nghiệm thu: chạy ship đồng thời retry/verify/đổi state phải serialize hoặc
  trả Busy trước mutation; không mất artifact, stale overwrite hay deadlock.
  Worker gọi operation bên trong vẫn giữ đúng một quyền sở hữu lock.

**Trạng thái: đã sửa.** Lock không re-entrant (flock gắn với open file
description), nên quyền sở hữu được truyền tường minh bằng `&TaskLockGuard`:
`state::with_task_lock(tasks_dir, id, held, f)` dùng lại guard khi `Some`, tự
acquire quanh `f` khi `None`. Bỏ hẳn cơ chế bỏ khoá theo `ZFORGE_HEADLESS`.

- Giữ suốt thao tác: `ship`/worker (worker gọi `ship::run_locked`), `verify`
  (load → chạy test → ghi kết quả).
- Load → sửa → ghi dưới khoá: `retry`, `approve`, `review --done`,
  `spec|testspec|plan|code --done`, MCP `get_prompt` sync, MCP `ship`.
- Dispatch phase (không `--done`) **không** giữ khoá suốt phiên agent — agent có
  thể tự gọi zforge/MCP trên cùng task. Orchestrator lấy khoá quanh từng lần ghi
  fallback, reload `.state.yaml` dưới khoá rồi mới áp swap.

Regression: `tests/task_mutation_test.rs` — `ship` (process thật, test chậm)
song song `retry` → retry trả Busy, artifact còn nguyên, ship kết thúc Verified;
`verify`/`review --done`/`approve`/`retry`/`ship` đều Busy khi task bị giữ và
không đổi state; chạy lại được sau khi nhả khoá. Unit test `with_task_lock`
trong `state/task_lock.rs`. Đã xác nhận các test này fail trên code trước sửa.

Hệ quả cần biết: [code.tmpl](../../templates/code.tmpl) bảo agent chạy
`zforge verify <ID>`; agent chạy dưới `ship` giờ nhận Busy (thông báo hướng dẫn
chạy test command trực tiếp). Trước đây lệnh này cũng đã lỗi ở lần code đầu
(`state is PlanReviewed, need Coded`) và race ở các lần retry sau.

Giới hạn còn lại: `task import` không lấy khoá (tạo task mới). Cửa sổ tranh
khoá giữa probe của `ship --async` và worker vẫn còn — thuộc [FIX-009](#fix-009).

### FIX-005

**[P1] Retry phải là rewind hợp lệ trong flow của task.**

- Hiện trạng: Full flow ở Imported, retry từ review nhảy lên Verified. Fixbug
  ở Coded, retry từ code reset thành PlanReviewed, vốn không thuộc flow đó.
- Vị trí: [retry](../../src/cli/retry.rs), `TaskState::reset_to` trong
  [state](../../src/state/mod.rs), [flow](../../src/state/flow.rs).
- Sửa: tính predecessor theo flow; kiểm tra phase, gate và hướng rewind trước
  khi backup/xóa artifact. State API cũng phải từ chối đích không hợp lệ.
- Nghiệm thu: Full, Fixbug, Docs, Spike xử lý đúng các phase được hỗ trợ; retry
  không thể tạo tiến độ chưa đạt. Input không hợp lệ giữ nguyên state/artifact.

**Trạng thái: đã sửa.** `retry::plan_retry(flow, current, phase)` tính đích là
predecessor *trong flow của task* của state mà phase tạo ra, danh sách artifact
suy từ các state còn lại của flow. Từ chối phase flow không có và rewind đi tới.
Toàn bộ kiểm tra chạy trước prompt/backup/xoá. `TaskState::reset_to` cũng tự
từ chối đích ngoài flow hoặc đi tới. Kết quả với Full flow giống hệt bảng cũ.

Regression: unit test `plan_retry` cho cả 4 flow; `tests/task_mutation_test.rs`
— fixbug retry `code` về `TestspecDone`; Full ở `Imported` retry `review` bị từ
chối với state file, artifact giữ nguyên và không tạo backup; Docs retry
`verify` bị từ chối.

### FIX-006

**[P1] Timeout phải giới hạn toàn bộ thời gian chờ cây process.**

- Hiện trạng: timeout 1 giây với descendant `sleep 3` giữ pipe vẫn mất khoảng
  3 giây. Runner chỉ kill process trực tiếp rồi join reader chờ EOF. Nguy cơ
  chờ vô hạn là suy luận; review chưa chạy trường hợp giữ pipe vô hạn.
- Vị trí: [agent spawn](../../src/orchestrator/spawn.rs),
  [test runner](../../src/runner/mod.rs).
- Sửa: quản lý descendant/process group và giới hạn drain/join; phối hợp với
  cancellation của background worker để không làm mất khả năng hủy cả job.
- Nghiệm thu: shell con/cháu giữ stdout hoặc stderr phải kết thúc trong timeout
  cộng grace period đã định; không để process con tiếp tục chạy. Kiểm tra cả
  agent runner, test runner, cancel và process exit bình thường.

**Trạng thái: đã sửa.** `src/process.rs::run_bounded` thay hai bản copy trong
agent runner và test runner. Mỗi child là leader của process group riêng.
Timeout: SIGTERM cả group → `KILL_GRACE` 2s → SIGKILL. Sau khi child chính thoát,
drain pipe tối đa `DRAIN_GRACE` 1s; còn descendant giữ pipe thì SIGKILL group;
mọi thứ còn trong group bị kill trước khi trả về. Giới hạn tối đa:
`timeout + 2s + 2×1s`. Timeout của test giờ là `OperationOutcome::Timeout`
(exit 124), không còn tính là Failed.

Tách group làm Ctrl-C và cancel không còn tự chạm tới child, nên thêm:
- handler SIGINT/SIGTERM/SIGHUP: chuyển tiếp tới mọi child group, chờ grace,
  SIGKILL phần còn lại, rồi re-raise để zforge vẫn chết như trước. Cần escalate
  vì shell không tương tác chạy `cmd &` với SIGINT bị ignore.
- worker ghi pgid các child vào `<job_dir>/child-pgids`; CLI và MCP cancel dùng
  chung `terminate_job_processes` (SIGTERM worker + child groups, 5s, SIGKILL),
  kể cả khi worker đã chết.

Regression: `tests/process_tree_test.rs` qua binary thật — verify timeout có
grandchild giữ pipe; exit bình thường để lại tiến trình nền; SIGINT vào zforge;
job cancel; job cancel với grandchild ignore SIGTERM. Mỗi case kiểm cả thời gian
và việc grandchild đã chết. Unit test `process` + test ở `orchestrator::spawn`.
Trên code cũ 4/5 case fail: verify treo >30s ở cả timeout lẫn exit bình thường,
SIGINT để lọt grandchild, và cancel cũ cũng rò rỉ — chỉ SIGKILL khi worker còn
sống, trong khi worker chết ngay khi nhận SIGTERM.

Giới hạn còn lại:
- Process tự tách khỏi group (`setsid`) mà vẫn giữ pipe: zforge thôi chờ và báo
  output không đầy đủ, nhưng không kill được nó.
- Timeout của *agent* vẫn đi qua exit 124 → fallback/lỗi, CLI trả exit 1 chứ
  chưa ra `Timeout` như test timeout.
- Windows: chỉ kill process trực tiếp; cancel vẫn chỉ đánh dấu.
- Legacy interactive dispatch (route 3) không đi qua `run_bounded`.

### FIX-007

**[P2] Ship phải giữ state do orchestrator vừa cập nhật.**

- Hiện trạng: primary exit 124, secondary exit 0; cost log ghi cả hai, nhưng
  ship lưu lại `active_agent: primary` và làm mất fallback history.
- Vị trí: [ship](../../src/cli/ship.rs), [orchestrator](../../src/orchestrator/run.rs).
- Sửa: dùng state hiện hành hoặc reload/merge có kiểm soát dưới cùng lock trước
  khi advance Coded; không lưu đè snapshot cũ sau dispatch.
- Nghiệm thu: sau ship, active agent, attempt/fallback history và cost evidence
  khớp runner thực tế; cả đường một lần chạy và nhiều iteration đều được kiểm tra.

**Trạng thái: đã sửa.** `ship` reload `.state.yaml` sau mỗi lần dispatch trước
khi advance `Coded` (`advance_to_coded_after_dispatch`), và mỗi iteration dựng
prompt từ state mới nhất. Orchestrator ghi fallback trên bản reload dưới khoá.

Regression: `tests/task_mutation_test.rs::ship_preserves_the_fallback_swap_recorded_during_dispatch`
(primary exit 124 → fallback exit 0; `active_agent`, `fallback_history` giữ
đúng, `assigned_agent` không đổi). Trên code cũ test này fail đúng như backlog:
`active_agent` bị trả về `primary`.

Giới hạn còn lại: test tự động mới phủ đường một lần chạy; đường nhiều iteration
dùng cùng helper nhưng chưa có test fallback riêng.

### FIX-008

**[P2] Resume ở Coded phải tiếp tục tự sửa trong budget.**

- Hiện trạng: `ship --max-iterations 3` với task Coded và test fail chỉ verify
  một lần, không gọi code agent. Nhánh return sớm bỏ qua vòng iteration.
- Vị trí: [ship](../../src/cli/ship.rs).
- Sửa: khi resume chỉ bỏ lần code đầu; verify fail và còn budget thì dispatch
  code với failure context, rồi verify lại. Làm rõ cách đếm iteration khi resume.
- Nghiệm thu: task Coded đã pass không chạy code thừa; task fail được sửa đến
  khi pass hoặc hết budget. Hết budget trả failure, không vượt giới hạn đã cấu hình.


**Trạng thái: đã sửa.** Quyết định: budget `--max-iterations N` đếm theo *số lần
verify*. Task chưa Coded: mỗi iteration là code → verify. Task đã Coded trở lên:
iteration 1 chỉ verify code hiện có; nếu fail và còn budget, các iteration sau là
code kèm failure context → verify. Tổng số lần gọi code agent không vượt N (khi
resume tối đa N−1). `orchestrator::verifier_loop::iterate_from(.., Start::Verify, ..)`.

Regression (`tests/verifier_loop_test.rs`): resume với test fail được sửa trong
budget (2 lần verify, 1 lần gọi agent); resume đã pass không gọi agent; resume
không bao giờ pass dừng đúng ở 3 lần verify / 2 lần gọi agent và trả lỗi hết
budget. Unit test cho `iterate_from`. Trên code cũ: resume verify một lần rồi trả
`Failed`, không gọi agent.

### FIX-009

**[P2] Lỗi khởi động worker phải có kết quả job có thể quan sát.**

- Hiện trạng: worker của queued job không lấy được task lock, exit 1 nhưng job
  vẫn queued. Reconciliation chỉ xử lý Running; submit có cửa sổ tranh lock.
- Vị trí: [worker](../../src/job/worker.rs), [lifecycle](../../src/job/lifecycle.rs),
  [async ship](../../src/cli/ship.rs).
- Sửa: ghi terminal outcome cho lỗi acquire/startup; ghi launch/PID đủ sớm và
  có recovery cho queued job không còn worker hợp lệ.
- Nghiệm thu: tranh lock, spawn thất bại và worker chết trước Running đều có
  trạng thái/lý do rõ; `job wait` kết thúc, không để queued vô hạn. Recovery
  không đánh dấu nhầm worker đang khởi động hợp lệ là đã chết.


**Trạng thái: đã sửa.**
- Worker: mọi lỗi sau khi đã load job (tranh task lock, `mark_running` lỗi) đều
  được ghi lên job (`failed`, "worker could not start: …") trước khi thoát.
- Submit: `cli::ship::submit_job` là đường duy nhất cho CLI `ship --async` và MCP
  `ship_async` (MCP trước đây có bản copy thiếu probe lock và xử lý lỗi spawn).
  Spawn lỗi → job `failed` với lý do. Spawn thành công → ghi PID vào
  `<job_dir>/launch.pid` — file riêng chỉ submitter ghi, nên không tranh ghi
  `job.yaml` với worker.
- Reconcile xét cả `queued`: PID đã launch mà chết → `failed`; chưa ghi launch
  sau `LAUNCH_GRACE` (30s) → `failed`; PID còn sống → giữ nguyên `queued` (worker
  đang khởi động hợp lệ).

Regression (`tests/background_job_test.rs`): worker thua lock → `failed` có lý
do; spawn thất bại → `failed`; worker chết trước Running → `failed`; worker còn
sống và job vừa tạo → vẫn `queued`; không launch quá grace → `failed`;
`zforge job wait` thoát (exit 1) thay vì poll tới timeout. Trên code cũ hai case
đầu fail đúng như backlog: job kẹt `queued`.

Giới hạn còn lại: cửa sổ giữa probe lock của submitter và acquire của worker vẫn
còn — giờ chỉ dẫn tới job `failed` có lý do rõ, không mất dấu. PID bị hệ điều hành
tái sử dụng có thể làm một worker đã chết trông như còn sống.

### FIX-010

**[P2] Serialize frontmatter thay vì ghép chuỗi command.**

- Hiện trạng: `/bin/sh -c "exit 0"` chạy pass nhưng command chèn vào YAML làm
  `set_frontmatter` lỗi parse; task không được advance.
- Vị trí: [verify report](../../src/cli/verify.rs).
- Sửa: dùng serializer cho dữ liệu frontmatter, giữ nguyên giá trị command.
- Nghiệm thu: round-trip command có dấu nháy, backslash và newline; báo cáo
  parse được, raw output được giữ, outcome/state phản ánh đúng test result.


**Trạng thái: đã sửa.** `VerifyReport` dựng frontmatter bằng `serde_yaml` (map có
thứ tự) thay vì ghép chuỗi; `tokens` và `model` được đưa vào ngay lúc render,
bỏ hai lượt `set_frontmatter` đọc lại file. Raw output nằm trong fence dài hơn
mọi chuỗi backtick trong output, nên output chứa ```` ``` ```` không làm vỡ báo cáo.
Test timeout được ghi `timed_out: true`.

Regression: unit test round-trip command có dấu nháy kép/đơn, backslash, newline,
`#`; output chứa ```` ``` ```` và dòng `---` giữ nguyên, frontmatter vẫn parse đúng.
`tests/cli_exit_code_test.rs::verify_with_a_quoted_command_passes_and_advances`
chạy đúng repro `/bin/sh -c "exit 0"` → exit 0, task `Verified`. Trên code cũ:
"existing frontmatter is malformed".

### FIX-011

**[P2] Ship phải xác định contract trước khi chạy flow không có verify.**

- Hiện trạng: Docs chạy code thành công, lên Coded rồi lỗi vì không có Verified.
  Spike có cùng đường gọi/flow thiếu Verified; chưa tái hiện riêng Spike.
- Vị trí: [ship](../../src/cli/ship.rs), [flow table](../../src/state/flow.rs).
- Sửa: chốt một hành vi công khai: hỗ trợ kết thúc theo gate của flow, hoặc từ
  chối ship trước dispatch nếu flow không hỗ trợ. Không tự thêm gate vào flow.
- Nghiệm thu: kiểm tra riêng Docs và Spike cùng Full/Fixbug; trường hợp bị từ
  chối không gọi agent hoặc thay state. Help và CLI/MCP/job cùng một contract.


**Trạng thái: đã sửa.** Quyết định: flow không có bước verify (Docs, Spike) thì
`ship` chạy code (nếu chưa Coded), lên `Coded` và kết thúc thành công — flow đã
hoàn tất. Không thêm gate vào flow. `--max-iterations` > 1 được báo là bỏ qua. Task
đã Coded thì là no-op thành công. CLI, MCP `ship` và job async cùng một contract;
help của `ship` đã cập nhật. Trường hợp bị từ chối là gate của flow (Spike chưa
có spec) → `Blocked`, không gọi agent, không đổi state.

Regression: Docs và Spike lên Coded không chạy test, không tạo verify.md; Spike
trước spec bị chặn; Docs đã Coded là no-op; Full/Fixbug vẫn chạy suite; job async
Docs thành công; MCP `ship` trên Docs (subprocess `zforge mcp` thật). Trên code cũ:
"phase 'verify' is not part of the 'docs' flow" sau khi agent đã code.

## Init và tích hợp tool

### FIX-012

**[P1] Đăng ký CodeGraph theo đúng MCP client.**

- Hiện trạng: `register_codegraph_mcp` chỉ ghi `.mcp.json`. Init Codex có
  `mcp_servers.zforge` nhưng thiếu CodeGraph; OpenCode tương tự với `mcp.zforge`.
  Cấu hình global có sẵn trên máy người dùng có thể che lỗi init sạch này.
- Vị trí: [init](../../src/cli/init.rs), các adapter trong
  [agent renderer](../../src/cli/init/agent_render.rs).
- Sửa: đăng ký qua cấu hình native từng client, merge bảo toàn server khác;
  render tên tool theo namespace client thực sự expose.
- Nghiệm thu: init sạch và init lại cho cả 3 target × shared/local; client nhận
  server, MCP handshake/tools/list thành công, search trả đúng symbol của đúng
  project. Thiếu dependency phải được báo rõ, không tính là tích hợp thành công.


**Trạng thái: đã sửa.** `init/tools.rs` đăng ký CodeGraph theo cơ chế native, theo
project, của từng client, ghim project bằng `codegraph serve --mcp --path <root>`
(không có `--path`, server tìm index từ cwd của client và báo "No CodeGraph
project is loaded" khi client khởi động ở nơi khác — đã kiểm với CodeGraph
0.9.4). Merge giữ nguyên server/key khác; init lại thay đúng một entry.

| Client | Cơ chế | Kiểm với CLI thật |
|---|---|---|
| Claude 2.1.273 | `claude mcp add --scope local codegraph -- …` | `claude mcp list` → Connected |
| OpenCode 1.14.41 | `mcp.codegraph` trong `<project>/opencode.json` | `opencode mcp list` → connected |
| Codex 0.145.0 | `[mcp_servers.codegraph]` trong `<project>/.codex/config.toml` | chỉ nạp khi project đã trust; sau khi trust `codex mcp list` hiện server |

Chạy đúng lệnh đã đăng ký từ cwd `/`: handshake OK, `tools/list` trả 10 tool,
`codegraph_search` trả đúng symbol của project. Không dùng `.mcp.json` cho Claude
nữa — server khai báo ở đó bị "Pending approval" và project không tự duyệt được,
kể cả qua `enabledMcpjsonServers` trong settings hay settings.local. Init báo rõ
khi codegraph chưa cài hoặc Codex chưa trust project; không tự trust thay người
dùng. `codex_config_path` giờ tôn trọng `$CODEX_HOME`.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do. Test `real_clients_load_the_codegraph_registration` (`#[ignore]`,
cần CLI thật) kiểm Claude/OpenCode; Codex kiểm tay vì cần bước trust.

Giới hạn còn lại: Codex yêu cầu người dùng trust project một lần. Server `zforge`
của Codex/OpenCode vẫn đăng ký ở config global.

### FIX-013

**[P1] Reference skills phải resolve đúng ở shared và local mode.**

- Hiện trạng: shared lưu skills ở `~/.zforge/skills`, nhưng hướng dẫn trỏ
  `.zforge/skills/...`, không có symlink bù. Cả 3 project Rust shared thử nghiệm
  đều thiếu 20/20 file được tham chiếu; local có đủ 20 file.
- Vị trí: [init](../../src/cli/init.rs), [AGENTS template](../../templates/AGENTS.md),
  [CLAUDE template](../../templates/CLAUDE.md), [language skills](../../src/cli/init/lang_skills.rs).
- Sửa: dùng cùng resolver cho config, prompt và file hướng dẫn; giữ rõ ownership
  của shared store và bản project-local. Không mặc định local path cho shared.
- Nghiệm thu: mọi reference được sinh đều mở được đúng nội dung, kể cả custom
  store path/project override được hỗ trợ. Không yêu cầu LLM tự đoán nơi lưu file.
  Discovery native skills là phần tiếp theo ở [IMP-004](./improvements.md#imp-004).


**Trạng thái: đã sửa.** `init/store_paths.rs` là resolver duy nhất cho config và
file hướng dẫn. Template (CLAUDE.md, AGENTS.md, zforge-readme.md) chỉ nhắc skill
qua `{{skills_dir}}`; bảng lang skills cũng vậy. Shared mode ghi đường dẫn tuyệt
đối tới store đang dùng (file tool của agent không tự mở `~`); config dùng `~/…`
khi store nằm trong HOME, tuyệt đối khi `$ZFORGE_HOME` ở chỗ khác.
`embedded::global_store_dir` giờ tôn trọng `$ZFORGE_HOME` như registry.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do. Trên code cũ: "20 of 20 skill references do not exist", đúng con
số của review.

Giới hạn còn lại: CLAUDE.md/AGENTS.md ở shared mode chứa đường dẫn tuyệt đối theo
máy — commit các file này vào repo dùng chung sẽ sai trên máy khác.

### FIX-014

**[P2] Dispatch phải áp dụng cấu hình named phase agent đã chọn.**

- Hiện trạng: orchestrator gọi Claude với `-p --model ...`, OpenCode với
  `run --model ...`; không có `--agent spec-agent`. Prompt capture không chứa
  definition đó. Legacy OpenCode có truyền `--agent`, legacy Claude chưa có.
- Vị trí: [orchestrator](../../src/orchestrator/run.rs),
  [registry defaults](../../src/registry/auto.rs), [legacy engine](../../src/prompt/engine.rs).
- Sửa: adapter chọn named agent được client hỗ trợ hoặc nạp cấu hình tương đương
  có kiểm chứng. Thống nhất purpose, model, skills và quyền theo phase giữa các
  đường CLI/MCP/slash command; không chỉ in tên agent trong log.
- Nghiệm thu: capture launch arguments và effective configuration cho từng
  phase; client nhận đúng agent/policy. Thiếu definition phải báo lỗi hoặc dùng
  fallback được khai báo. Model profile và subprocess riêng không được tính
  thành bằng chứng native subagent. Phần Codex mở rộng ở [IMP-004](./improvements.md#imp-004).


**Trạng thái: đã sửa.** `orchestrator::agent_args` thêm `--agent <phase>-agent` cho
claude và opencode khi file định nghĩa tồn tại (claude 2.1 và opencode 1.14 đều có
cờ `--agent`); dùng chung cho orchestrator và đường streaming. Fallback được khai
báo: thiếu file thì chạy không `--agent` và cảnh báo chỉ rõ file thiếu. Codex
không có named agent — dùng profile `zforge_<phase>` như trước.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do. Trên code cũ argv là `claude -p --output-format json --model …`,
không có `--agent`.

Giới hạn còn lại: mới kiểm argv; chưa kiểm client thật áp đúng instructions/model
của định nghĩa (cần gọi model).

### FIX-015

**[P1] Default runner của project phải khớp lựa chọn init.**

- Hiện trạng: init `--agent codex`, task không có assigned agent, chạy spec
  non-interactive gọi Claude. Init vẫn seed Claude; `DEFAULT_RUNNER` là Claude.
- Vị trí: [init](../../src/cli/init.rs), [dispatch helper](../../src/cli/dispatch_helper.rs).
- Sửa: lưu default runner của project, xác định precedence với task override
  và registry. Init nhiều target cần có quy tắc default rõ trong config/help.
- Nghiệm thu: init từng target rồi import/chạy task không gán agent gọi đúng
  runner; task override được tôn trọng. Foreground/background/MCP nhất quán;
  runner không có sẵn không âm thầm chuyển sang agent khác ngoài policy.


**Trạng thái: đã sửa.** `runner.default` trong `.zforge/config.yaml`, chọn lúc init
(`init/runner.rs`): `--default-runner` (phải là client được scaffold) → client
duy nhất → với `--agent all`, client đầu tiên có trên `PATH` theo thứ tự
claude → codex → opencode. Registry chỉ seed đúng các client được chọn (trước đây
luôn thêm claude). Dispatch bỏ hằng `DEFAULT_RUNNER`; đường streaming dùng đúng
runner thay vì auto-detect; runner không có trong registry là lỗi có hướng dẫn,
không lặng lẽ chuyển sang client khác.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do. Mỗi client: init rồi `spec` task không gán agent chỉ gọi đúng
client đó.

Giới hạn còn lại: config viết tay thiếu `runner.default` dùng `claude`
(`FALLBACK_RUNNER`).

### FIX-016

**[P2] RTK setup phải dùng target phù hợp.**

- Hiện trạng: cả 6 cấu hình init gọi `rtk init -g`, nhánh mặc định cho Claude.
  RTK 0.37.2 hỗ trợ `--codex` và `--opencode` nhưng init không truyền.
- Vị trí: [ensure_rtk](../../src/cli/init.rs).
- Sửa: dùng flag đúng theo target/version được hỗ trợ; kiểm tra cấu hình tạo
  ra, thông báo rõ khi chưa hỗ trợ thay vì chỉ dựa trên exit code installer.
- Nghiệm thu: kiểm tra argv và registration thực tế cho mỗi client; init lại
  không nhân đôi hook hoặc ghi đè config ngoài phần zForge quản lý. Không công
  bố giảm token nếu chưa có phép đo tương ứng.


**Trạng thái: đã sửa.** Claude `rtk init -g`, Codex `-g --codex`, OpenCode
`-g --opencode` (cờ này cài thêm plugin OpenCode *cùng* phần Claude, nên với
`all` chỉ chạy `--opencode` + `--codex`). Kiểm với rtk 0.37.2 trong HOME tạm: mỗi
dạng ghi đúng file của client (`~/.claude/RTK.md`, `~/.codex/RTK.md` + tham chiếu
trong `~/.codex/AGENTS.md`, `~/.config/opencode/plugins/rtk.ts`) và chạy lại
không đổi file nào.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do.

Giới hạn còn lại: chưa đo hiệu quả token; hook Claude vẫn phụ thuộc bước patch
settings.json mà rtk hỏi người dùng.

### FIX-017

**[P2] Refresh adapter phải bảo toàn model do người dùng cấu hình.**

- Hiện trạng: hướng dẫn sửa `models.yaml` rồi `init --force`; init lại ghi đè
  template trước khi render. Marker model tùy chỉnh đã mất trong thử nghiệm.
- Vị trí: [init](../../src/cli/init.rs), [hướng dẫn refresh](../../templates/AGENTS.md).
- Sửa: tách refresh file sinh tự động khỏi reset config người dùng; xác định
  ownership từng loại file. Refresh phải render từ config hiện hành.
- Nghiệm thu: model tùy chỉnh tồn tại sau refresh và xuất hiện trong agent/profile;
  config và memory không bị reset như tác dụng phụ. Reset có chủ đích phải là
  thao tác được mô tả rõ, với phạm vi và cách khôi phục cụ thể.


**Trạng thái: đã sửa.** Ownership rõ theo key (`init/config_file.rs`):
`runner.default`, `paths.agents`, `paths.skills` do init quản lý và cập nhật mỗi
lần init; mọi key khác của config.yaml chỉ ghi khi tạo mới. `models.yaml` và
`memory/` không bao giờ bị ghi đè, kể cả với `--force` — giờ `--force` chỉ nghĩa
là làm mới file sinh ra. Agent definitions render lại từ models.yaml hiện hành.

Regression: `tests/init_e2e_test.rs` chạy binary thật, mọi vị trí config (HOME,
ZFORGE_HOME, CODEX_HOME, XDG_CONFIG_HOME, CLAUDE_CONFIG_DIR) trỏ vào thư mục tạm,
`PATH` chỉ có stub ghi argv. Trên code cũ 14/15 test fail đúng lý do. Trên code cũ model tùy chỉnh bị ghi đè sau `init --force`.

## Review nhánh sửa lỗi (21–22/09/2026)

Review toàn bộ nhánh `fix/operation-outcome-plumbing` so với `main` (3 reviewer
song song + chạy full suite). Mỗi finding được đọc lại trong code trước khi nhận.
Tất cả đã sửa, mỗi mục một commit và regression test.

| ID | Ưu tiên | Vấn đề | Sửa | Regression |
|---|---|---|---|---|
| FIX-018 | P1 | `job cancel` khi job còn `queued` chỉ đổi nhãn; worker vẫn chạy ship và ghi đè `cancelled` bằng kết quả của nó | `lifecycle::cancel_job` dùng chung CLI/MCP, lấy `launch.pid` khi chưa có `worker_pid`; trạng thái kết thúc là cuối cùng, `mark_running` từ chối job đã hủy | `background_job_test`: `cancel_stops_a_queued_worker…`, `a_cancelled_job_stays_cancelled` |
| FIX-019 | P2 | Fingerprint kế thừa `GIT_DIR`/`GIT_WORK_TREE` (chạy từ git hook) → tính trên repo khác, evidence cũ được nhận | Bỏ các biến trong `git rev-parse --local-env-vars` khỏi mọi lệnh git của fingerprint | `evidence_test`: `inherited_git_environment…` |
| FIX-020 | P2 | Init lại xoá mọi comment trong `config.yaml` (round-trip `serde_yaml`) | Sửa trực tiếp dòng của key được quản lý; parse lại để đối chiếu, layout lạ thì fallback | unit test `config_file` (comment, section thiếu, flow style) |
| FIX-021 | P2 | `init --force` thay nguyên `.claude/settings.json`, mất permission/hook/env của user (có từ trước nhánh) | Merge: giữ mọi key/entry theo thứ tự, thêm allow của zForge còn thiếu, chỉ bỏ tên CodeGraph < 0.9; `serde_json/preserve_order` | unit `claude_settings`, e2e `refresh_keeps_user_claude_settings` |
| FIX-022 | P3 | Fingerprint bỏ sót sửa đổi trong submodule đã checkout và nội dung sau symlink trỏ ra ngoài project | `evidence::linked`: fingerprint đệ quy submodule, hash nội dung đích symlink ngoài project; không có hai loại này thì giữ tree hash cũ | `evidence_test`: submodule, symlink file, thư mục 3000 file |
| FIX-023 | P3 | Signal handler dùng `signal()`: signal thứ hai trong lúc forward chạy chồng handler, zForge chết bởi signal sau | `sigaction` với `sa_mask` chặn cả SIGINT/SIGTERM/SIGHUP; bỏ signal đang chờ trước khi re-raise | `process_tree_test`: `a_second_signal_during_forwarding…` |
| FIX-024 | P3 | Dọn skill cũ xoá cả skill của user có tên `zforge-*` | Chỉ xoá khi `SKILL.md` có `metadata.generated-by: zforge` | e2e `refresh_removes_stale_zforge_skills_only` |
| FIX-025 | P3 | `doctor` fingerprint lại cho từng task Verified (mỗi lần ~6 lệnh git) | `evidence::status_against` + một fingerprint lười cho cả lượt | `doctor_test` hiện có |
| FIX-026 | P3 | Test `verify_timeout…` flaky dưới tải: timeout 3s kill script trước khi ghi pid | Đọc pidfile sau khi zForge thoát; chưa có pid thì chạy lại với 6s, 12s | chính test đó, full suite 2 lượt liên tiếp |

Còn để lại, rủi ro thấp hoặc chỉ là câu chữ: message MCP `ship` luôn ghi "up to N
iterations"; bảng con `[mcp_servers.codegraph.env]` tự thêm bị bỏ lại khi init lại
Codex; khoảng tái dùng pgid rất hẹp giữa reap và kill; child đăng ký đúng lúc cancel
đọc danh sách group; tên index tạm đoán được (tối đa DoS qua `.lock`).

## Thứ tự triển khai và điều kiện đóng lỗi

1. **Kết quả và gate:** FIX-001, FIX-002, FIX-003.
2. **State và giới hạn:** FIX-004, FIX-005, FIX-006; phối hợp FIX-007/FIX-009.
3. **Init đúng target:** FIX-012, FIX-013, FIX-015; hoàn thiện FIX-014/FIX-016/FIX-017.
4. **Các đường hồi phục và flow:** FIX-007 đến FIX-011 còn lại.

Đây là thứ tự ưu tiên, không bắt buộc đợi refactor toàn bộ mới sửa được từng lỗi.
Các mục P1 trên đường runtime/adapter được dùng phải đạt trước khi tuyên bố
[Mốc B của workflow](./workflow.md) chạy tự chủ đáng tin cậy. Intake và thiết kế
có thể tiếp tục trong lúc sửa nền tảng.

Chỉ đóng một mục khi có bản sửa, regression test thể hiện hành vi đúng, kiểm
chứng trên candidate cụ thể và ghi giới hạn còn lại. Test stub không thay thế
kiểm tra client thật khi nghiệm thu phụ thuộc discovery/registration của client.
Thay đổi quyết định sản phẩm còn mở, như contract ship của short flow, đi qua
intake triển khai; lỗi kỹ thuật thông thường được tự xử lý trong phạm vi đã giao.

Xem [cải tiến và ma trận kiểm chứng](./improvements.md), [tổng quan v1.5](./README.md).
