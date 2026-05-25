# Task-Level Agent Assignment & Global Orchestration

> **Phân chia triển khai:** Tài liệu này được chia thành 3 PR độc lập để giảm rủi ro:
>
> - **PR 1 — Global Registry + Auto-Register on Init:** §2.1, §3.1, §3.3, §3.4, §5 (tools `project_list`, `global_status`, `switch_project`)
> - **PR 2 — Task-Level Agent Assignment:** §2.2, §3.2
> - **PR 3 — Fallback Orchestration:** §4 (toàn bộ logic điều phối + fallback)
>
> Mỗi PR phải pass acceptance criteria tương ứng trước khi merge PR kế tiếp.

## 1. Mục tiêu (Objective)

Nâng cấp framework `zforge` từ một công cụ quản lý pipeline cục bộ (local scope) thành một **Global Project Orchestrator (Hệ điều hành quản lý dự án AI toàn cục)**. Tính năng cốt lõi bao gồm: Quản lý danh sách dự án tập trung, gán AI Agent cố định theo từng Task (Task-Level Agent Assignment), và tự động kích hoạt Agent/chuyển đổi dự phòng (Fallback).

## 2. Kiến trúc lưu trữ (Storage Architecture)

### 2.1. Global Registry Store

Mọi thông tin toàn cục lưu tại thư mục `~/.zforge/` (đã được sử dụng bởi `zforge install`).

- **Đường dẫn (Path):** `~/.zforge/registry.yaml`
  - **Lý do tách khỏi `config.yaml`:** Tên `config.yaml` đã được dùng cho project-local config (`.zforge/config.yaml`). Dùng `registry.yaml` để tránh nhầm lẫn về scope.
- **Cấu trúc dữ liệu (Schema):**

```yaml
# Project hiện tại của session (persisted, đọc khi không có --project flag)
current_project: "ecommerce-app"

projects:
  - name: "ecommerce-app"
    path: "/Users/username/projects/ecommerce-app"
    # Auto-register metadata
    registered_at: "2026-05-21T10:23:00Z"
    registered_by: "init"          # "init" | "manual" (project add)
    # Optional: override agent config cho riêng project này
    agent_overrides:
      claude:
        args: ["code", "--model", "opus"]
  - name: "payment-service"
    path: "/Users/username/projects/payment-service"
    registered_at: "2026-05-19T08:00:00Z"
    registered_by: "manual"

# Định nghĩa lệnh thực thi của từng Agent ở mức global
# CẢNH BÁO BẢO MẬT: Các giá trị command/args được thực thi như user hiện tại.
# zforge phải warn nếu `command` không tìm thấy trong $PATH lúc đăng ký.
agents:
  claude:
    command: "claude"
    args: ["code", "--non-interactive"]
  codex:
    command: "codex"
    args: ["--profile", "zforge_active"]
  opencode:
    command: "opencode"
    args: ["run"]

# Chính sách fallback toàn cục (override được ở task level)
fallback_policy:
  max_retries: 2                     # Số lần thử fallback tối đa cho một task
  cooldown_seconds: 30               # Chờ giữa các lần retry
  # Exit codes được coi là "retryable" (trigger fallback)
  retryable_exit_codes: [2, 124, 137]
  # Regex match stderr — nếu khớp, trigger fallback bất kể exit code
  retryable_stderr_patterns:
    - "(?i)rate.?limit"
    - "(?i)quota.?(exceeded|exhausted)"
    - "(?i)token.?exhausted"
    - "\\b429\\b"
    - "(?i)api.?network.?error"
```

#### 2.1.1. Validation rules cho `project add` và auto-register

- `path` phải tồn tại, là directory, và chứa subdirectory `.zforge/` (cho `project add`); với auto-register từ `init`, `.zforge/` được tạo trong cùng lệnh nên check sau khi scaffold xong.
- `name` phải unique trong `projects[]`.
- `name` phải match regex `^[a-zA-Z0-9_-]{1,64}$`.
- Đường dẫn được lưu dưới dạng absolute (canonical) — resolve symlink ở thời điểm `add`/`init`.
- Nếu một entry đã tồn tại với cùng `path` (canonical) nhưng khác `name` → conflict; xem §3.4 cho xử lý.

### 2.2. Task State Extension (Local Scope)

Bổ sung các thuộc tính mới vào file trạng thái cục bộ của từng task để phục vụ việc điều phối.

- **Đường dẫn (Path):** `.zforge/tasks/<ID>/.state.yaml`
- **Các trường bổ sung (New Fields):**

```yaml
# Agent gốc được assign — IMMUTABLE sau khi import. Phục vụ audit trail.
assigned_agent: "claude"

# Agent dự phòng — cấu hình tại import time.
fallback_agent: "codex"

# Agent đang hoạt động — có thể bị mutate bởi orchestrator khi fallback.
# Nếu null/missing => bằng assigned_agent.
active_agent: "claude"

# Lịch sử fallback — append-only, mỗi lần swap thêm một entry.
fallback_history:
  - timestamp: "2026-05-21T10:23:00Z"
    from: "claude"
    to: "codex"
    reason: "exit_code:124"          # hoặc "stderr_match:rate.?limit"
    phase: "code"
```

- `assigned_agent`, `fallback_agent`: bắt buộc khi tạo task qua `--agent`/`--fallback`. Nếu không cung cấp, fallback default về Agent đã được `init` (xem `cli/init/detect.rs`).
- `Flow` của task được preserve khi fallback — agent mới kế thừa cùng Flow.

---

## 3. Chi tiết các Lệnh CLI mới (New CLI Commands)

### 3.1. Nhóm lệnh `zforge project` (Global Management)

- `zforge project add <PATH> --name <NAME>`
  - Validate theo §2.1.1; lỗi nếu vi phạm.
  - `registered_by: "manual"`.
- `zforge project list [--json]`
  - Hiển thị bảng (default) hoặc JSON, kèm cột `registered_by` để phân biệt auto vs manual.
- `zforge project remove <NAME> [--purge]`
  - Default: chỉ xóa entry khỏi registry; `.zforge/` trong project vẫn nguyên.
  - `--purge`: xóa luôn `.zforge/` trên đĩa (yêu cầu confirm trừ khi có `--yes`).
- `zforge project switch <NAME>`
  - Set `current_project` trong registry (CLI parity với MCP `switch_project`).
- `zforge project current`
  - In tên project hiện tại (tiện cho shell prompt / scripting).

### 3.2. Mở rộng lệnh `zforge task import`

Bổ sung 2 flag mới để gán Agent ngay khi tạo task:

```bash
zforge task import TASK-001 --title "Fix login" --agent claude --fallback codex
```

Quy tắc:

- `--agent` phải match một key trong `agents{}` của registry; lỗi nếu không có.
- `--fallback` optional; nếu thiếu, không có cơ chế fallback (task fail thì fail).
- `--agent` và `--fallback` không được trùng nhau.

### 3.3. Nhóm lệnh Dashboard / Toàn cục

- `zforge status --global [--json] [--timeout-ms 2000]`
  - Quét tất cả `projects` trong registry.
  - **Per-project timeout** (default 2s) — project nào quá hạn bị skip kèm warning, không làm crash toàn bộ.
  - **Đọc bằng atomic read** (`fs::read_to_string` + parse; bỏ qua project có YAML lỗi, in lỗi vào stderr).
  - Output gom tất cả task active (state `< Reviewed` trong Flow tương ứng) thành một bảng.

### 3.4. Auto-register on `zforge init`

Khi user chạy `zforge init` ở một thư mục mới, project tự động được đăng ký vào `~/.zforge/registry.yaml`. Mục tiêu: bỏ bước thủ công `project add` — `init` xong là project đã có mặt trong global dashboard.

#### Flow

1. `init` scaffold `.zforge/` như hiện tại.
2. Sau khi scaffold xong, gọi `registry::auto_register(cwd, name)`.
3. `name` resolve theo thứ tự:
   - Flag `--name <NAME>` nếu user truyền.
   - `basename(cwd)` (mặc định), sanitize theo regex `^[a-zA-Z0-9_-]{1,64}$` — ký tự không hợp lệ thay bằng `-`.
4. Set `current_project = <name>` nếu registry trống hoặc user truyền `--switch`.

#### Xử lý conflict

- **Cùng path, đã đăng ký:** No-op, in info `"already registered as <existing_name>"`. Không lỗi.
- **Khác path, cùng name:** Auto-suffix `-2`, `-3`, ... cho đến khi unique. In warning kèm tên cuối cùng.
- **Cùng path, khác name (user đổi tên thư mục):** Update entry với name mới, giữ `registered_at` cũ. In info.

#### Flags

- `--no-register`: bỏ qua auto-register (cho CI, ephemeral worktree, hoặc khi user không muốn).
- `--name <NAME>`: override tên auto-detect.
- `--switch`: set `current_project` thành project mới ngay sau khi register.

#### Lỗi non-fatal

Nếu register thất bại (registry corrupt, không quyền write `~/.zforge/`), `init` vẫn complete với exit code 0 nhưng in warning ra stderr. Init đã thành công về mặt local scaffold — registry là enhancement, không phải requirement cứng.

---

## 4. Logic Vận Hành & Điều Phối Agent (Orchestration Logic)

Khi người dùng hoặc một Automation Agent (như Hermes) gọi lệnh thực thi một phase (Ví dụ: `zforge code TASK-001`):

### 4.1. Phân tích Ngữ cảnh (Context Resolution)

1. Nếu có `--project <NAME>`: tra registry, đổi CWD sang `path`.
2. Nếu không: dùng CWD hiện tại; nếu CWD nằm trong một project đã đăng ký, dùng project đó; nếu không, fallback về `current_project` của registry.

### 4.2. Xác định Agent

1. Đọc `.state.yaml`, lấy `active_agent` (fallback về `assigned_agent` nếu missing).
2. Tra `agents{}` trong registry; áp `agent_overrides` từ project entry nếu có.
3. Nếu agent không tồn tại trong registry → exit code 2, message rõ ràng.

### 4.3. Thực thi & Fallback

1. Sinh prompt qua `build_context_for_phase()` (reuse logic hiện tại).
2. Spawn child process bằng `std::process::Command`.
3. Capture stdout + stderr; chờ exit code.
4. **Quyết định fallback:**
   - Nếu exit code == 0 → success, đi tiếp.
   - Nếu exit code nằm trong `retryable_exit_codes` HOẶC stderr khớp một pattern trong `retryable_stderr_patterns` → trigger fallback.
   - Mọi exit code khác (test fail, compile fail, user cancel via SIGINT) → propagate lỗi nguyên trạng, KHÔNG fallback.
5. **Khi trigger fallback:**
   - Append entry vào `fallback_history` với `from`, `to`, `reason`, `phase`, `timestamp`.
   - Set `active_agent = fallback_agent`. **KHÔNG mutate `assigned_agent`.**
   - Đếm `fallback_history.len()`; nếu ≥ `max_retries` → exit code 3, message rõ ràng, không retry nữa.
   - Sleep `cooldown_seconds`.
   - Re-run phase với agent mới; truyền cùng context (`build_context_for_phase()` đảm bảo các artifacts trước đó đã có sẵn dưới dạng `/file` refs).
   - Lần fallback kế tiếp (nếu lỗi tiếp) yêu cầu `fallback_agent` thứ ba — hiện tại chỉ hỗ trợ một bước fallback. Nếu cả `active_agent` mới cũng lỗi và đã hit `max_retries` → terminal failure.

### 4.4. Concurrency

- Mọi mutate `.state.yaml` và `registry.yaml` phải qua atomic write (`write_to_temp → rename`).
- `status --global` scan chỉ đọc; chấp nhận snapshot có thể stale vài giây.
- Auto-register trong `init` dùng advisory file lock (`flock`) quanh `registry.yaml` để hai `init` chạy đồng thời không corrupt.

---

## 5. Cập nhật MCP Layer (Model Context Protocol)

Tools đăng ký theo convention hiện tại (không prefix `zforge_`):

- `project_list` → JSON danh sách project + `current_project`.
- `project_add(name, path)` → register project mới.
- `project_remove(name, purge?)` → xóa khỏi registry.
- `switch_project(name)` → đổi `current_project`.
- `global_status(timeout_ms?)` → JSON tất cả active task toàn hệ thống.
- `task_import` (mở rộng): nhận thêm tham số `agent`, `fallback`.

Tất cả tools delegate trực tiếp về `cli::*` (không tạo code path mới — invariant đã ghi trong `CLAUDE.md`).

---

## 6. Tiêu chí Nghiệm thu (Acceptance Criteria / Tests)

### PR 1 — Global Registry + Auto-Register on Init

- [ ] `zforge project add` lưu vào `~/.zforge/registry.yaml` với `registered_by: "manual"`.
- [ ] `project add` reject duplicate name (exit code ≠ 0, message rõ).
- [ ] `project add` reject path không tồn tại / không có `.zforge/`.
- [ ] `project add` reject name không match regex.
- [ ] `project remove` không `--purge` giữ nguyên `.zforge/` trên đĩa.
- [ ] `project remove --purge` xóa `.zforge/` sau khi confirm.
- [ ] `project switch` và MCP `switch_project` đổi `current_project` thống nhất.
- [ ] `status --global` skip project corrupt mà không crash; in warning ra stderr.
- [ ] `status --global --timeout-ms 100` skip project quá chậm.
- [ ] `zforge init` ở thư mục mới tự động thêm entry vào registry với `registered_by: "init"`.
- [ ] `zforge init` ở thư mục đã đăng ký (cùng path) → no-op, in info, exit 0.
- [ ] `zforge init` với name trùng (khác path) → auto-suffix `-2`, in warning.
- [ ] `zforge init --no-register` không động vào registry.
- [ ] `zforge init --name custom-name` ghi đè tên auto-detect.
- [ ] `zforge init --switch` set `current_project` ngay.
- [ ] `zforge init` không quyền write `~/.zforge/` → exit 0, warning ra stderr, scaffold local vẫn thành công.
- [ ] Hai `zforge init` chạy song song trên hai thư mục khác nhau → cả hai entries đều có trong registry (file lock work).

### PR 2 — Task-Level Agent Assignment

- [ ] `task import --agent opencode` → `.state.yaml` có `assigned_agent: opencode`, `active_agent: opencode`.
- [ ] `task import --agent X` với X không tồn tại trong registry → reject.
- [ ] `task import --agent claude --fallback claude` → reject (trùng).
- [ ] Khi chạy phase, đúng binary của OpenCode được spawn (verify qua mock hoặc `which`-style stub).

### PR 3 — Fallback Orchestration

- [ ] Giả lập exit code 124 → `active_agent` đổi sang `fallback_agent`, `fallback_history` có entry mới, phase được retry.
- [ ] Giả lập exit code 1 (test fail) → KHÔNG fallback, propagate lỗi.
- [ ] Giả lập stderr chứa "rate limit exceeded" với exit code 1 → fallback trigger.
- [ ] `assigned_agent` IMMUTABLE qua fallback (so sánh trước/sau).
- [ ] Fallback hit `max_retries` → terminal failure, exit code 3.
- [ ] Cooldown được respect (đo thời gian giữa 2 lần spawn).
- [ ] Flow của task không đổi sau fallback.
- [ ] Agent mới nhận đủ artifacts của các phase trước qua `/file` refs.

---

## 7. Rủi ro & Mitigations

| Rủi ro | Mitigation |
| ------ | ---------- |
| Arbitrary command execution qua `agents.*.command` | Warn khi `command` không có trong `$PATH` lúc register; document trong README. |
| Race condition khi nhiều `zforge` chạy song song trên cùng task | Atomic write `.state.yaml`; advisory file lock (`flock`) quanh mutate. |
| Race condition khi nhiều `init` chạy song song | `flock` quanh `registry.yaml` đảm bảo các entries không ghi đè nhau. |
| Fallback loop vô hạn | `max_retries` cap; sau khi hit, terminal failure rõ ràng. |
| Registry corruption | Atomic write; backup `.bak` trước mỗi mutate. |
| `--global` scan chậm với 100+ projects | Per-project timeout; parallel scan (rayon) nếu cần sau benchmark. |
| Auto-register lộ path private của user khi share registry | `registry.yaml` là per-user, không commit; document trong README, gợi ý gitignore nếu user symlink registry vào dotfiles repo. |
