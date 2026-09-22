# MOC-B — Breakdown

## Phase

1. **Nền** — record của run và worktree: độc lập, test được mà không cần agent.
2. **Thực thi** — hợp đồng từ snapshot, `Flow::Contract`, vòng thực thi có budget.
3. **Vận hành** — status/list/cancel/retry/clean, amendment, knowledge.
4. **Nghiệm thu** — benchmark IMP-006 trên một leaf task thật.

## Task và dependency

| Task | Việc | Requirement | Phụ thuộc |
|---|---|---|---|
| TASK-001 | Record của run: `run.yaml`, `events.jsonl`, phát lại trạng thái, reconcile interrupted | REQ-003, REQ-007 | — |
| TASK-002 | Worktree: tạo từ commit baseline trên branch riêng, liệt kê, xóa | REQ-002 | — |
| TASK-003 | Hợp đồng: đọc manifest, kiểm hash, dựng prompt; `Flow::Contract`; từ chối task có dependency | REQ-001, REQ-004, REQ-009 | — |
| TASK-004 | Vòng thực thi: `zforge run`, work dir tường minh cho agent/test/fingerprint, sự kiện và trace, budget | REQ-004, REQ-005, REQ-006 | TASK-001, TASK-002, TASK-003 |
| TASK-005 | `run status/list/cancel/retry/clean`, chạy nền qua job | REQ-007, REQ-008 | TASK-001, TASK-004 |
| TASK-006 | Amendment: agent ghi `changes/CHANGE-nnn.md`, lần chạy dừng `blocked` | REQ-010 | TASK-004 |
| TASK-007 | Knowledge `verified` từ record của run; `result.md` | REQ-011 | TASK-004 |
| TASK-008 | Nghiệm thu: benchmark một leaf task thật qua handover | REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006 | TASK-004, TASK-005, TASK-006, TASK-007 |

Theo REQ-009, chính Mốc B chưa chạy được task có dependency, nên các task này
được làm tuần tự theo cách v1 (người phát triển hoặc `zforge ship`), không qua
`zforge run`. Từ TASK-004 trở đi, các task sau có thể được chạy thử bằng chính
`zforge run` như một kiểm chứng phụ.

## Interface dùng chung

- `run::record::RunEvent` — enum sự kiện (created, started, attempt, verified,
  verify_failed, blocked, failed, cancelled), serialize snake_case; TASK-004,
  TASK-005, TASK-006, TASK-007 cùng đọc và ghi qua đây.
- `run::record::RunState` — trạng thái phát lại (§6.1) + chi phí đã dùng + lần
  verify gần nhất (candidate).
- `run::contract::Contract` — manifest đã kiểm, task, danh sách snapshot đã
  đọc; TASK-004 nhận từ TASK-003.
- Work dir tường minh: tham số `work_dir: &Path` cho spawn agent, lệnh test và
  fingerprint. TASK-004 thêm; code v1 truyền `project_root`, nên hành vi v1 giữ
  nguyên.

## Kiểm chứng tích hợp

- Toàn bộ `cargo test` (hiện 667 test) pass sau mỗi task; không test v1 nào bị
  sửa kỳ vọng.
- Test e2e dùng stub claude phát lại stream thật: `zforge run` trên một handover
  thật trong git repo tạm đi tới `verified`. Working tree chính không đổi (so
  `git status` trước và sau); branch và worktree tồn tại; `events.jsonl` có đủ
  sự kiện; `zforge run status` và `knowledge index` khớp nhau.
- Các ca lỗi bằng stub: hash lệch, dependency, hết vòng, hết budget, amendment,
  kill giữa chừng rồi reconcile, cancel.
- TASK-008: benchmark với Claude thật (haiku), có budget, trên một leaf task của
  một intake thật, và lưu trace vào `target/bench/`.

## Câu hỏi còn mở
