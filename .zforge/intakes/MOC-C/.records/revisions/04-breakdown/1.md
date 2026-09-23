# MOC-C — Breakdown

## Phase

1. **Output và điểm xuất phát** — niêm phong output của run verified thành
   commit; task có dependency xuất phát từ output đó. Sau phase này chạy từng
   task bằng `--task` được.
2. **Tích hợp và trạng thái feature** — run tích hợp; trạng thái feature suy từ
   record.
3. **Vòng chạy feature** — một lệnh cho cả handover, khóa, resume, nền, MCP;
   tái dùng qua handover.
4. **Knowledge và nghiệm thu** — ba mức verified / integration_verified /
   integrated; kịch bản đầu cuối.

## Task và dependency

| Task | Việc | Requirement | Phụ thuộc |
|---|---|---|---|
| TASK-001 | Niêm phong output: `verified` ghi commit có fingerprint bằng candidate | REQ-002 | — |
| TASK-002 | Điểm xuất phát: `start` trong `run.yaml`, worktree từ commit/merge của dependency, bỏ từ chối `depends_on`, từ chối khi dependency chưa verified | REQ-002, REQ-003 | TASK-001 |
| TASK-003 | Run tích hợp: `kind: integration`, merge output các task lá, lệnh tích hợp từ breakdown đã pin | REQ-004 | TASK-002 |
| TASK-004 | Trạng thái feature suy từ record; `run status <HANDOVER>` | REQ-005, REQ-007 | TASK-002, TASK-003 |
| TASK-005 | Vòng chạy feature: `zforge run <HANDOVER>`, khóa, resume, nền, cancel, MCP | REQ-001, REQ-005, REQ-008 | TASK-004 |
| TASK-006 | Tái dùng task qua handover: khóa hợp đồng, event `reused` | REQ-006 | TASK-005 |
| TASK-007 | Knowledge: `integration_verified`, `integrated` | REQ-009 | TASK-003 |
| TASK-008 | Nghiệm thu đầu cuối bằng stub: năm dấu hiệu thành công của 01-outcome | REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009 | TASK-005, TASK-006, TASK-007 |

Đồ thị: 001 → 002 → 003 → 004 → 005 → 006; 003 → 007; 005, 006, 007 → 008.
TASK-007 độc lập với 004–006: nếu 005 bị chặn, 007 vẫn làm được.

Chính intake này có dependency. Sau TASK-002, task tiếp theo chạy được bằng
`zforge run HANDOVER --task`; sau TASK-005, phần còn lại chạy được bằng
`zforge run HANDOVER` — dùng như kiểm chứng phụ.

## Interface dùng chung

- `run::record::RunEvent::Verified { candidate, commit }` — `commit` do TASK-001
  thêm; TASK-002 (xuất phát), TASK-003 (merge), TASK-006, TASK-007 đọc.
- `run::record::RunMeta { kind, start }` — `Start { commit, from: [Source] }`,
  `Source { task, run, commit }`. TASK-002 thêm `start`, TASK-003 thêm `kind`.
  Thiếu `start` nghĩa là baseline; thiếu `kind` nghĩa là `task`.
- `run::start::{plan, create}` — TASK-002 viết; TASK-003 dùng lại để dựng
  worktree tích hợp (baseline + merge).
- `run::feature::FeatureState` — TASK-004 viết; TASK-005 (vòng chạy), CLI và MCP
  đọc. Hàm thuần, không I/O.
- `run::reuse::contract_key` — TASK-006; khóa gồm hash 4 stage, file task, file
  task của dependency bắc cầu, commit baseline.

## Kiểm chứng tích hợp

Chạy ở gốc repo sau khi mọi task xong:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Cộng với test đầu cuối của TASK-008 (nằm trong `cargo test`): handover
A ← B ← C cùng D độc lập, dùng stub claude, trong git repo tạm; working tree
chính không đổi; mọi view (`run status`, `knowledge index`) khớp record. Không
test Mốc B nào bị sửa kỳ vọng.

## Câu hỏi còn mở
