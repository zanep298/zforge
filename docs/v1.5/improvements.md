# v1.5 — Cải tiến độ tin cậy và tích hợp agent/tool

**Trạng thái: đang triển khai. Claude Code là client làm trước; mỗi mục ghi *Tiến độ* và *Chưa làm*.**

Kế hoạch bổ sung cho [workflow intake và thực thi](./workflow.md), dựa trên
[backlog lỗi và bằng chứng review](./bug-fixes.md). Mục tiêu là giúp người dùng
biết init đã cấu hình được gì, runtime thực sự chạy gì, và kết quả nào đã được
kiểm chứng. Những hướng sửa cụ thể giữ ID `FIX-*`; các thay đổi nền tảng dưới đây
dùng `IMP-*`, tránh tính cùng một lỗi thành hai công việc độc lập.

## Phần đang hoạt động cần giữ lại

- CodeGraph thật hỗ trợ init/index/MCP và 10 tên tool trong template hiện tại;
  search fixture đã tìm đúng symbol. Registration/allowlist Claude đã khớp tên.
- Server CodeGraph có autosync khi watcher bật; không kết luận mọi lần sửa code
  đều bắt buộc index thủ công. Cần kiểm tra đúng project và độ mới của dữ liệu.
- OpenCode nhận file `spec-agent` được sinh, gồm model và prompt. Claude có
  definition Markdown và `/zforge` có hướng dẫn giao việc cho named subagent.
- Orchestrator có subprocess theo phase và model routing; Codex có model profile.
  Các cơ chế này có giá trị riêng, chưa chứng minh native subagent được sử dụng.
- Checklist local có thể được agent đọc như tài liệu. Thiếu native discovery
  không có nghĩa agent chắc chắn không bao giờ đọc nội dung checklist.

## Danh mục cải tiến

| ID | Kết quả cần đạt | Liên quan |
|---|---|---|
| [IMP-001](#imp-001) | CLI/MCP/worker cùng contract operation | FIX-001 đến FIX-005, FIX-007 đến FIX-011 |
| [IMP-002](#imp-002) | State/artifact phục hồi được khi bị ngắt | FIX-002, FIX-004, FIX-007, FIX-009 |
| [IMP-003](#imp-003) | Regression và release checks bảo vệ hành vi bên ngoài | Tất cả FIX |
| [IMP-004](#imp-004) | Adapter native đúng cho skills/agents/MCP từng client | FIX-012 đến FIX-017 |
| [IMP-005](#imp-005) | Báo cáo readiness tool dựa trên kiểm tra thực tế | FIX-012, FIX-013, FIX-014, FIX-016 |
| [IMP-006](#imp-006) | Trace và benchmark chứng minh dùng đúng tool trong một task | Toàn bộ đường intake → execution |

### IMP-001

**Thống nhất operation giữa CLI, MCP và worker.**

Business operation trả outcome có cấu trúc: thành công, thất bại, bị chặn,
timeout hoặc bị hủy; kèm state/evidence cần thiết. CLI render và ánh xạ exit
code, MCP serialize response, worker ghi job outcome. Không để renderer quyết
định gate hoặc wrapper bỏ mất failure.

Lock, kiểm tra flow/gate và mutation dùng chung một đường. Caller giữ lock phải
truyền ownership rõ khi gọi operation lồng nhau. Cho phép refactor tăng dần qua
các fix, không yêu cầu rewrite runtime trước khi xử lý lỗi P1.

**Nghiệm thu:** cùng một input cho CLI/MCP/job tạo outcome và state tương đương;
MCP không phát log thường ra stdout; lỗi nội bộ không biến thành success tại
boundary. Các test phải bảo vệ contract bên ngoài, không chỉ gọi helper nội bộ.

### IMP-002

**Lưu state an toàn và phục hồi được khi execution bị ngắt.**

[TaskState::save](../../src/state/mod.rs) đang ghi trực tiếp bằng `std::fs::write`.
Đây là rủi ro thấy từ source; review chưa fault-inject mất điện hoặc crash giữa
lần ghi. Chọn atomic replacement phù hợp nền tảng, với temp file, sync và rename;
xác định thêm cách phát hiện/khôi phục khi state và artifact cập nhật dở dang.
Atomic write một file không tự tạo transaction cho nhiều file.

Mỗi verification cần nhận diện candidate/input được kiểm tra. Thay code, retry
hoặc đổi input không được tiếp tục sử dụng pass evidence cũ như kết quả hiện hành.
Job startup/recovery phải giữ lịch sử attempt và lý do dừng.

**Tiến độ:**
- Atomic write: `TaskState::save` và mọi artifact qua `writer::write_file` dùng
  `state::write_atomic` (temp + fsync + rename). Test tất định: handle mở trước
  khi save vẫn đọc được bản cũ đầy đủ.
- **Evidence gắn candidate** (`src/evidence/`): `verify.md` ghi `candidate` — git
  tree hash của working tree (gồm thay đổi chưa commit và file untracked không
  bị ignore), tính bằng index tạm nên không đụng index/staging của người dùng;
  loại `.zforge/` và `.codegraph/`; chỉ lấy subtree khi project là thư mục con
  của repo. Chụp trước và sau khi chạy suite, gắn với tree *sau* (file do suite
  sinh ra không làm evidence tự cũ); khác nhau thì ghi `candidate_note`.
  `review --done` chỉ nhận evidence *hiện hành*: pass + đúng test command đã
  cấu hình + candidate trùng tree hiện tại. Ngoài git: không có fingerprint,
  review vẫn đi tiếp với cảnh báo.
- Fingerprint không bị điều hướng bởi `GIT_DIR`/`GIT_WORK_TREE` kế thừa (FIX-019),
  và bao gồm submodule đã checkout cùng nội dung sau symlink trỏ ra ngoài project
  (`evidence::linked`, FIX-022). Project không có hai loại này giữ tree hash cũ.
- Lịch sử từng lần verify: `<task>/verify-history.jsonl`.
- Job: trạng thái kết thúc là cuối cùng; cancel dừng cả worker còn `queued`
  (FIX-018).
- Thứ tự ghi: verify.md → history → state. Crash giữa chừng không tạo Verified
  thiếu report (có test: ghi report lỗi → task vẫn Coded).

Regression: `tests/evidence_test.rs` (15, binary thật trong git repo thật) và
unit test `evidence::candidate`. Trên code cũ: review chấp nhận evidence sau khi
code đã sửa và evidence của command khác.

Chưa làm: recovery khi nhiều artifact cập nhật dở dang (không có transaction
nhiều file), fault injection ở mức rename/fsync. `status` chưa hiển thị evidence
cũ (tính fingerprint mỗi lần status tốn kém với `status --global`). Process
group của tiến trình con được ghi *sau* khi spawn (`process::persist`, có fsync):
worker bị SIGKILL trong khe vài mili giây đó để lại tiến trình con không ai dừng.

**Nghiệm thu:** fault injection tại các bước ghi/rename/cập nhật artifact không
để state bị cắt cụt hoặc tạo Verified thiếu evidence. Recovery phân biệt được
run đã hoàn thành, đã dừng và trạng thái cần kiểm tra lại; không tự giả định pass.
Định dạng record/schema là quyết định của v1.5, không mặc định dùng ADR v2.

### IMP-003

**Regression theo hành vi và kiểm tra trước release.**

Tại baseline review, workflow [release](../../.github/workflows/release.yml)
build/publish nhưng chưa chạy suite test. Chưa kiểm tra branch protection hay
cấu hình remote, nên không suy luận rằng GitHub hoàn toàn không có kiểm soát.

Thêm CI cho PR/push và điều kiện test pass trước publish. Chuyển các tình huống
tái hiện trong backlog thành test bền vững trong repository, ưu tiên exit status,
job outcome, stdout MCP, lần chạy thứ hai, short flow và mutation đồng thời.
Không giữ assertion chỉ vì nó được gọi là legacy nếu nó xác nhận hành vi sai.

Tách lớp kiểm tra rõ ràng:

| Lớp | Chứng minh được | Không thay thế được |
|---|---|---|
| Unit và subprocess với stub | Gate, state, argv, budget, failure propagation | Native discovery và hành vi LLM |
| CLI/tool thật, không gọi model | Parse config, nhận agent/skill/MCP, truy vấn fixture | LLM chọn và dùng đúng capability |
| Task nhỏ với LLM thật | Hành vi trên scenario và phiên bản được ghi nhận | Bảo đảm đúng trên mọi task/model |

**Tiến độ:** `.github/workflows/ci.yml` chạy fmt, clippy `-D warnings` và test
trên PR/push; `release.yml` gọi CI và chỉ publish khi pass. Các tình huống tái
hiện của FIX-001 → FIX-026 là test trong repository (binary thật, stub client).

Chưa làm: CI chưa tách test đã chạy / bị skip / thiếu dependency;
`real_clients_load_the_codegraph_registration` vẫn `#[ignore]` nên CI không có
bằng chứng client thật; chưa có lớp "task nhỏ với LLM thật" (IMP-006).

**Nghiệm thu:** CI phân biệt test đã chạy, bị skip và thiếu dependency; không
tính test real-agent tự return là bằng chứng tích hợp thật. Release chỉ quảng
cáo client/version/feature đã có coverage phù hợp, giữ artifact/log cần để review.

### IMP-004

**Adapter native cho skills, named agents và MCP theo client.**

Hiện tại `.zforge/skills/*.md` là checklist, chưa có package `SKILL.md` và metadata
để native discovery. OpenCode thật trả catalog rỗng trong project local đã init.
Codex `.codex/agents/*.md` là prompt reference; model profile không tự đăng ký
native custom agent. Đây là khoảng trống tích hợp, không phải bằng chứng runtime
v1.5 hoặc native subagent đã được triển khai.

Adapter cần có trách nhiệm rõ:

| Thành phần | Hành vi đề xuất |
|---|---|
| Skills | Xuất `SKILL.md` và metadata đúng format/path client; kiểm tra discovery, giữ reference tới nguồn/version |
| Shared/local | Dùng resolver thống nhất; bản local override và shared source có ownership rõ |
| Phase | Phân biệt skill bắt buộc và skill gợi ý; nạp/tham chiếu đúng skill theo phase và ghi bằng chứng sử dụng |
| Named agent | Render định dạng client hỗ trợ và chọn agent khi launch; kiểm tra effective instructions, model, tools/quyền |
| Model profile | Xử lý riêng với named agent, không dùng số profile làm số subagent |
| MCP | Merge config native theo client; kiểm tra namespace tool, server và project root |
| Version | Phát hiện capability/version; báo unsupported rõ ràng nếu adapter không hỗ trợ |
| Refresh | Render từ config người dùng hiện hành; chỉ cập nhật phần được zForge quản lý |

Theo tài liệu đối chiếu trong review, Codex discovery skills dùng
`.agents/skills/<name>/SKILL.md`, Claude dùng `.claude/skills/<name>/SKILL.md`,
OpenCode có `.opencode/skills/<name>/SKILL.md`. Codex native custom agents trong
tài liệu hiện hành dùng TOML; cần xác minh phiên bản CLI hỗ trợ trước khi chọn
format, không chuyển file Markdown thành native agent chỉ bằng đổi tên.

Nguồn đối chiếu: [Codex skills](https://learn.chatgpt.com/docs/build-skills),
[Codex subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents),
[Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli),
[Claude skills](https://code.claude.com/docs/en/skills),
[Claude subagents](https://code.claude.com/docs/en/sub-agents),
[OpenCode skills](https://opencode.ai/docs/skills),
[OpenCode MCP](https://opencode.ai/docs/mcp-servers).
Các nguồn này cần được đối chiếu lại với version được chọn khi implement.

**Tiến độ (Claude Code — client chính):** `src/cli/init/claude_skills.rs` là catalog
duy nhất cho skill native:
- Mỗi checklist → `.claude/skills/zforge-<tên>/SKILL.md` với `name`, `description`
  viết theo hướng "khi nào dùng" (Claude dựa vào đây để tự nạp), `metadata` ghi
  nguồn và version. Ghi ở cả shared lẫn local mode; init lại xoá `zforge-*` không
  còn trong catalog, không đụng skill khác của người dùng.
- Phân biệt bắt buộc / gợi ý: checklist của phase (và `<lang>-patterns`,
  `<lang>-testing` cho phase code) nằm trong `skills:` của `<phase>-agent` — Claude
  nạp nguyên nội dung khi agent khởi động. Các skill còn lại để model tự gọi theo
  description.
- Bảng skill trong CLAUDE.md sinh từ cùng catalog, dùng tên skill thay vì đường
  dẫn file.
- Unit test chặn việc thêm checklist vào store mà không có mục catalog.

Kiểm với Claude Code thật: `claude plugin validate` báo `✔ Validation passed` cho
`.claude/skills` (20 skill cho project Rust) và `.claude/agents`. `zforge doctor`
dùng validator này để lên mức *recognized* cho skills và agents, và kiểm mọi skill
được preload đều tồn tại.

Chưa kiểm: model có tự gọi skill không preload hay không, và subagent có thực sự
nạp `skills:` trong phiên chạy — cả hai cần gọi model (IMP-006). Codex/OpenCode chưa
làm.

**Nghiệm thu:** init sạch ở 6 tổ hợp client/mode; client thật liệt kê đúng native
skills/agents/server được hỗ trợ, launcher áp dụng đúng phase configuration.
Global config có sẵn không được che thiếu sót init. Custom model, server ngoài
zForge và nội dung người dùng vẫn được giữ sau init lại/refresh.

### IMP-005

**Readiness của tool phải phân biệt đã tạo file, đã nhận và đã gọi được.**

Sau init hoặc refresh, báo cáo từng capability theo mức bằng chứng:

1. Binary/version hiện có và được adapter hỗ trợ.
2. Config/reference đã ghi đúng nơi, parse được, không thiếu file.
3. Client nhận capability qua catalog hoặc kiểm tra native tương ứng.
4. Smoke test không gọi model thành công, khi capability có phép kiểm tra này.
5. Có trace sử dụng trong task thật, nếu đã chạy benchmark.

Không coi exit code installer hoặc file marker là đủ để xác nhận các mức sau.
Thiếu dependency, unsupported client và bước chưa kiểm tra phải hiện rõ. Tool
optional thiếu không nhất thiết chặn cả project; tool bắt buộc trong hợp đồng
task thì phải làm readiness thất bại trước dispatch.

CodeGraph kiểm tra handshake/tools/list/search trong đúng project, rồi kiểm tra
thay đổi fixture có được phản ánh theo cơ chế sync đang bật. RTK kiểm tra setup
cho target thực tế. Caveman hiện chỉ kiểm tra file hook tồn tại; cần kiểm tra
registration và invocation trước khi tuyên bố hook hoạt động. Review chưa chạy
installer hoặc đo hiệu quả token của Caveman.

**Tiến độ (Claude Code, client chính):** `zforge doctor [--json]` (`src/cli/doctor/`).
Mỗi mục báo mức cao nhất *đã kiểm được*: missing / broken / present / configured
/ recognized (CLI của Claude xác nhận) / working (smoke test, không gọi model), và
`not_checked` cho phần không kiểm được khi không tương tác.

| Mục | Kiểm gì | Mức cao nhất |
|---|---|---|
| claude | `claude --version` | working |
| runner | `runner.default` có trong registry, binary trên PATH | configured |
| agents | 5 định nghĩa phase: có, parse được, `name` khớp, có `model`; `claude plugin validate` | recognized (Claude không có lệnh liệt kê subagent) |
| skills | native `zforge-*` skills có đủ, mọi skill preload tồn tại; `claude plugin validate` | recognized |
| zforge / codegraph MCP | `claude mcp get`: đăng ký, connected (health check), codegraph ghim đúng project | working |
| rtk hook | có trong settings (user/project/local) *và* viết lại `git status` thật | working |
| caveman hook | đã đăng ký, script tồn tại | configured |
| workspace trust | `hasTrustDialogAccepted` cho project hoặc thư mục cha gần nhất trong `.claude.json`; chưa trust thì `claude -p` bỏ allowlist của project | configured (tuỳ chọn) |
| evidence | task Verified có evidence khớp code hiện tại (IMP-002) | working |

Mục bắt buộc (claude, runner, agents) lỗi → exit 1; mục tuỳ chọn cảnh báo kèm lệnh
sửa. Chạy thật với claude 2.1.278 / codegraph / rtk trong HOME tạm: init mới báo
zforge MCP chưa đăng ký và rtk chưa gắn hook; sau khi đăng ký thì cả hai MCP
connected và rtk working. Regression: `tests/doctor_test.rs` (11, stub `claude`
trả từng trạng thái MCP, stub `rtk` hoạt động/không hoạt động).

Chưa làm: Codex/OpenCode; CodeGraph mới kiểm qua health check của `claude mcp
get` — chưa gọi `tools/list`/search đúng root, chưa kiểm sửa fixture được phản
ánh; Caveman chưa kiểm invocation; mức 5 (trace sử dụng trong task thật) thuộc
IMP-006.

**Nghiệm thu:** tình huống binary thiếu, config lỗi, server không chạy, sai root,
native catalog rỗng hoặc hook chưa đăng ký tạo kết quả cụ thể và hướng xử lý;
không báo tất cả đã sẵn sàng. Tên lệnh/chế độ chẩn đoán là thiết kế triển khai
còn mở, chưa phải CLI hiện có.

### IMP-006

**Trace có thể review và benchmark một task có giới hạn.**

Ghi runner thực tế, client version, phase agent, model, skill/version đã nạp,
MCP server/tool đã gọi, project root, attempt/fallback, thời gian và outcome.
Tách cấu hình dự kiến khỏi invocation quan sát được. Nếu client không expose
đủ trace, ghi rõ thiếu bằng chứng thay vì khẳng định skill/tool đã được dùng.

Benchmark một task nhỏ từ init đến bàn giao, với contract và budget cố định:
đọc code qua CodeGraph khi scenario yêu cầu, nạp skill bắt buộc, thực hiện phase
được giao, chạy verification, sửa lỗi có kiểm soát và xuất kết quả. Bổ sung trường
hợp thiếu skill/server, test fail và interruption để xác nhận không báo pass giả.

**Tiến độ (Claude Code):**
- Claude chạy với `-p --output-format stream-json --verbose` (default registry;
  hai default cũ `-p`, `-p --output-format json` được migrate). Mỗi lần spawn
  ghi một dòng vào `<task>/trace.jsonl` (`src/trace/`), tách *expected* (named
  agent, model, skill preload của agent, CodeGraph khi project có index) khỏi
  *observed* (client version, model, session, trạng thái MCP, lời gọi
  tool/Skill/subagent — cả của subagent —, permission bị từ chối, cảnh báo
  stderr về config bị bỏ, `result`). So khớp hai phần sinh *findings* chia ba
  loại: infrastructure / agent choice / run. Phần Claude không báo được ghi vào
  `not_observable`: stream chỉ có catalog skill, không cho biết `skills:` có
  được preload vào agent hay không. Runner khác và lần chạy interactive có bản
  ghi `unavailable`, không có bản ghi rỗng kiểu "ổn".
- `zforge trace <ID> [--json]`: task → từng phase/attempt → runner/agent/model
  (cấu hình và thực tế)/skill/MCP/tool/kết quả/findings → từng lần verify với
  candidate → evidence còn khớp code hay không.
- Ca lỗi chạy trong CI bằng stub phát lại stream thật của Claude 2.1.278
  (`tests/fixtures/claude/`, đã làm sạch): thiếu agent/skill/server, server
  không connected, model khác, bị từ chối quyền, allowlist bị bỏ, chạy bị cắt
  ngang, output không phải stream (`trace_test`, unit `trace::claude`).
- Benchmark thật: `cargo test --test bench_claude_test -- --ignored --nocapture`.
  Kịch bản cố định (project shell, `add` bị trừ, flow Fixbug, spec → testspec →
  ship tối đa 2 vòng verify), haiku, `--max-budget-usd 0.40` mỗi spawn. Đạt khi
  mọi phase có trace và kết thúc, không có finding infrastructure, verify cuối
  pass trên candidate đang có. Lượt đạt ngày 22/09/2026: 3 phase, $0.53, verify
  pass, evidence current.

Benchmark tìm ra ngay hai lỗi, đã sửa kèm regression: (1) mọi stream-json có
`rate_limit_event`, nên pattern fallback `rate.?limit` khớp với mọi lần chạy
thành công và phase bị fail — giờ stream được đối chiếu trên lỗi của `result`,
không trên framing (`fallback::scan_text`); (2) cảnh báo "Ignoring N
permissions.allow … not trusted" bị tính là lỗi dù chạy headless bypass quyền.

Quan sát từ lượt đạt, chưa xử lý:
- Không phase nào gọi CodeGraph dù đã connected (agent choice). Project shell
  có thể không được CodeGraph index; cần kịch bản ngôn ngữ được hỗ trợ.
- Workspace chưa trust thì `claude -p` bỏ toàn bộ allowlist của project. Chạy
  foreground không TTY (MCP, CI) sẽ bị từ chối Bash. Đã thêm check
  `workspace trust` vào `doctor` (trust của thư mục cha có hiệu lực cho thư mục
  con — đã kiểm với 2.1.278 khi thư mục con chưa có entry riêng).
- Project không nhận ra ngôn ngữ bị init gán `rust` (có chủ đích từ trước:
  `empty_dir_falls_back_to_rust`), nên phase code preload skill Rust cho
  project shell.
- Verify báo `0 tests` với output shell: runner không đếm được test, pass chỉ
  dựa trên exit code.

Chưa làm: Codex/OpenCode; kịch bản benchmark thứ hai (ngôn ngữ CodeGraph hỗ
trợ, feature nhiều file); benchmark ca lỗi với model thật (đang dùng stub).

**Nghiệm thu:** người review truy vết được requirement → phase → runner/skill/tool
→ thay đổi → kiểm chứng trên candidate. Tách lỗi hạ tầng/tool, lỗi lựa chọn của
agent và lỗi sản phẩm. Ghi model/version, budget, scenario và giới hạn suy rộng;
một lượt benchmark thành công không chứng minh mọi task sẽ dùng đúng tool.

## Ma trận kiểm chứng tối thiểu

| Phạm vi | Tình huống bắt buộc |
|---|---|
| Init | Claude/Codex/OpenCode × shared/local; sạch, chạy lại, custom config |
| Runner | Default theo project, task override, runner thiếu, fallback |
| Skills | Reference resolve, native discovery, skill bắt buộc thiếu, local override |
| Agent | Phase/model/purpose/quyền thực tế; definition thiếu; version unsupported |
| CodeGraph | Registration, namespace, handshake, search đúng root, dữ liệu sau sửa fixture |
| Hooks | RTK đúng client, init lại không duplicate, Caveman registered/invoked khi áp dụng |
| Refresh | Model/server/config/memory người dùng được giữ; generated output cập nhật |
| Runtime | Pass/fail/blocked/timeout/cancel; reverify, retry, resume, fallback |
| Đồng thời/phục hồi | Tranh lock, worker chết, ghi state/artifact bị ngắt |
| Transport/flow | CLI/MCP/job đồng nhất; Full/Fixbug/Docs/Spike đúng contract |

Ma trận là phạm vi nghiệm thu đề xuất; không phải bảng các test đã pass.

## Gắn với lộ trình v1.5

1. **Mốc 0 — Củng cố nền tảng:** sửa P1 và các P2 trên đường chạy được chọn;
   bổ sung phần IMP-001/IMP-002 cần cho các fix và regression ở IMP-003.
2. **Mốc A — Intake:** có thể thiết kế và kiểm chứng trải nghiệm song song;
   ghi tool/skill/agent bắt buộc và phương pháp kiểm chứng trong hợp đồng task.
3. **Trước khi nghiệm thu Mốc B:** adapter/readiness của client được chọn đạt
   IMP-004/IMP-005; chạy bounded benchmark IMP-006 và các failure case liên quan.
4. **Mốc C — Nhiều task:** mở rộng coverage dependency/tích hợp sau khi một task
   đã có bằng chứng. Không dùng việc sinh nhiều agent/skill file để chứng minh
   execution nhiều task đã sẵn sàng.

Mỗi mục chỉ hoàn thành khi có code, kiểm chứng và evidence đúng phạm vi. Lựa chọn
storage, schema, CLI và mức tái sử dụng runtime vẫn tuân theo các quyết định mở
trong [workflow](./workflow.md); kế hoạch này không tự áp dụng thiết kế v2.
