# MOC-C — Solution

## Luồng xử lý

```text
zforge run HANDOVER-001                                        (REQ-001)
  → lấy khóa của handover; khóa đang bị giữ thì từ chối        (REQ-008)
  → với từng task theo thứ tự manifest:
       trạng thái suy từ record của các run                     (REQ-007)
       verified / reused            → bỏ qua
       dependency không verified    → chờ; dependency chặn → bị chặn (REQ-005)
       có run tái dùng được ở handover trước → run `reused`     (REQ-006)
       run gần nhất interrupted     → retry (run mới, retry_of) (REQ-008)
       còn lại → tạo run:
         start = baseline | commit của dependency | merge của chúng (REQ-002)
         chạy như Mốc B; `verified` niêm phong output thành commit
  → mọi task verified/reused → run tích hợp:                   (REQ-004)
       worktree từ baseline, merge output của các task lá
       chạy lệnh "Kiểm chứng tích hợp" đã pin; không gọi agent
       verified (candidate + commit) | failed
  → tóm tắt feature; knowledge index                            (REQ-009)
```

## Component và interface

- `src/run/record.rs`
  - `RunEvent::Verified` thêm `commit: Option<String>` (output đã niêm phong).
  - `RunEvent::Reused { at, from_run, from_handover, candidate, commit }`: một
    run đi thẳng từ `ready` tới `verified` mà không gọi agent.
  - `RunMeta` thêm `kind: task | integration` (mặc định `task`) và
    `start: Option<Start>` với `Start { commit, from: Vec<Source> }`,
    `Source { task, run, commit }`. Thiếu `start` = xuất phát từ baseline.
    Trường mới `#[serde(default, skip_serializing_if)]` nên `run.yaml` cũ đọc
    được và run không dependency ghi ra y như Mốc B.
- `src/run/output.rs` (mới) — `seal(run)`: sau khi test pass, commit mọi thay
  đổi còn lại vào branch của run, tính lại fingerprint, phải bằng candidate vừa
  test; trả về commit.
- `src/run/start.rs` (mới) — tính và tạo điểm xuất phát: không dependency →
  baseline; một → commit của nó; nhiều → `git worktree add` tại commit đầu rồi
  `git merge --no-ff` lần lượt các commit còn lại; xung đột → `merge --abort`,
  lỗi liệt kê task và file xung đột.
- `src/run/feature.rs` (mới) — hàm thuần
  `FeatureState::derive(manifest, deps, runs) -> FeatureState`: mỗi task là
  `Waiting { on }`, `Running(run)`, `Verified { run, commit }`,
  `Reused { run, from }`, `Blocked { by }`, `Stopped(run, final state)`; cộng
  trạng thái tích hợp. Không đọc gì ngoài record và manifest.
- `src/run/integrate.rs` (mới) — run `kind: integration`: worktree
  `zforge/<INTAKE>/integration/<RUN>` từ baseline, merge output của các task lá,
  chạy lệnh tích hợp qua `process::run_bounded` trong worktree đó.
- `src/run/reuse.rs` (mới) — khóa hợp đồng của một task:
  SHA-256 của hash 4 stage + hash file task + hash file task của mọi dependency
  bắc cầu + commit baseline. Tìm run `verified` ở handover trước của cùng intake
  có cùng khóa.
- `src/run/feature_ops.rs` (mới) — vòng chạy feature, khóa, chạy nền, cancel.
  Tiến trình của vòng chạy (pid, log) ở `.zforge/runs/features/<INTAKE>/<HANDOVER>/`,
  chỉ là sổ ghi tiến trình, không chứa trạng thái.
- `src/run/contract.rs` — bỏ lệnh từ chối `depends_on`; thêm
  `integration_commands()` đọc từ `04-breakdown.md` đã pin.
- `src/intake/knowledge.rs` — `integration_verified { run, candidate }` và
  `integrated { commit }`.
- CLI `src/cli/run.rs`: `zforge run <HANDOVER> [--async]` (không `--task`),
  `run status <HANDOVER>`, `run cancel <HANDOVER>`, `run log <HANDOVER>`,
  `run wait <HANDOVER>`. Phân biệt bằng tiền tố `RUN-` / `HANDOVER-`.
- MCP `src/mcp/v15.rs`: `run_start` có `task` không bắt buộc; `run_status`,
  `run_cancel`, `run_log` nhận id handover. Không thêm tool mới, `FORBIDDEN`
  giữ nguyên.

## Quyết định bắt buộc

- Output của một task là commit do zforge niêm phong ngay khi test pass, ghi
  trong event `verified`; fingerprint của commit đó phải bằng candidate đã test.
  Commit agent tạo sau đó không phải output. Run `verified` của Mốc B không có
  commit thì không dùng làm dependency được; báo rõ và yêu cầu chạy lại task đó.
- Điểm xuất phát đi theo đúng đồ thị dependency: không dependency → baseline;
  một → commit của nó; nhiều → merge. Không xếp chồng tuyến tính theo thứ tự
  manifest, để task độc lập không nhận code không liên quan và một task bị chặn
  không chặn task độc lập.
- Kiểm chứng tích hợp là một run riêng, `kind: integration`, cùng dạng record,
  không gọi agent, không tốn budget. Lệnh là khối code đầu tiên trong mục
  "Kiểm chứng tích hợp" đã pin, mỗi dòng không rỗng là một lệnh chạy lần lượt;
  không có khối code thì dùng `project.test_command`, và kết quả ghi rõ đã dùng
  cái nào.
- Tái dùng chỉ khi khóa hợp đồng trùng tuyệt đối, kể cả commit baseline. Tái
  dùng được ghi thành một run có event `reused`, để mọi trạng thái vẫn đến từ
  record.
- Không có file trạng thái feature. Mọi view feature là hàm của record các run
  cộng manifest.
- Mỗi (handover, task) có một khóa `flock` giữ suốt lần chạy; vòng chạy feature
  giữ thêm khóa của handover. Khóa thay cho việc tin rằng người dùng không chạy
  hai lần.
- `integrated` = commit của run tích hợp là tổ tiên của branch baseline hiện tại
  (`git merge-base --is-ancestor`). Suy ra khi dựng knowledge, không lưu.

## Gợi ý triển khai

- `FeatureState::derive` là hàm thuần nhận dữ liệu đã đọc, để unit test đủ mọi
  tổ hợp chặn/chờ/tái dùng không cần git.
- Merge trong worktree dùng `-c user.name=zforge -c user.email=zforge@localhost`
  như `commit_leftovers`; tách hàm git dùng chung thay vì chép lần thứ ba.
- Vòng chạy feature gọi `execute::execute` trong cùng process cho từng run (như
  foreground Mốc B); chế độ nền là một worker ẩn `zforge run-worker --handover`.
- Cancel feature: tín hiệu tới worker của vòng chạy; handler hiện có ghi
  `cancelled` cho run đang chạy rồi vòng chạy dừng, task còn lại để `waiting`.

## Phương án đã cân nhắc

- **Xếp chồng tuyến tính** (mỗi task xuất phát từ task trước trong manifest):
  không cần merge, nhưng task độc lập nhận code không liên quan, và một task bị
  chặn chặn mọi task sau nó — trái REQ-005. Loại.
- **Tích hợp là bước cuối của run task cuối**: không cần `kind`, nhưng run đó
  mang hai nghĩa, candidate của nó không phải tree tích hợp khi có nhiều lá, và
  retry task cuối sẽ chạy lại tích hợp. Loại.
- **Không tái dùng, handover mới chạy lại hết**: đơn giản hơn nhiều, nhưng một
  amendment nhỏ ở task cuối bắt trả lại chi phí mọi task trước — trái mục tiêu
  §12 "amendment chỉ vô hiệu phần bị ảnh hưởng". Giữ, với điều kiện trùng tuyệt
  đối để không có tái dùng sai.
- **File trạng thái feature** (`feature.yaml`): đọc nhanh hơn, nhưng thành nguồn
  sự thật thứ hai có thể lệch khỏi record run. Loại.
- **Người dùng ghi nhận `integrated`**: thêm một quyết định cho việc git đã trả
  lời được. Loại; squash/rebase merge được báo là `integration_verified`, đúng
  với những gì biết chắc.
- **Để agent sửa khi tích hợp fail**: không có hợp đồng nào cho phần "giữa các
  task"; sửa ở đâu là quyết định về hợp đồng. Loại; đi qua amendment.

## Giả định và bằng chứng

- `git worktree add <dir> <commit>` + `git merge --no-ff <commit>` trong
  worktree không đụng checkout chính: Mốc B đã dùng worktree theo cách này
  (`run/worktree.rs`), merge chỉ ghi vào branch của worktree.
- Fingerprint (`evidence::fingerprint`) của worktree không đổi khi commit mọi
  thay đổi, vì nó hash tree của working tree, không phụ thuộc trạng thái index;
  sẽ kiểm bằng test ở TASK-001 thay vì giả định.
- Manifest đã giữ `tasks` theo thứ tự dependency và readiness đã chặn vòng và
  dependency ngoài phạm vi (`readiness.rs::dependency_order`), nên vòng chạy
  feature không phải tự kiểm lại đồ thị; vẫn kiểm lại từ snapshot đã pin để
  không tin manifest mù quáng.
- Lệnh tích hợp đến từ file do người dùng chốt ở terminal và được pin bằng hash
  — cùng mức tin cậy với `project.test_command`.

## Câu hỏi còn mở
