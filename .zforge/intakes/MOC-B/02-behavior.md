# MOC-B — Behavior

Mỗi tình huống ghi lệnh người dùng chạy, những gì họ thấy được và REQ mà nó phục
vụ. `RUN-007` là ví dụ về ID của một lần chạy; `HANDOVER-001` pin TASK-002 với
budget $3.00 và tối đa 3 vòng verify.

## Tình huống

### Bình thường

1. **Chạy một task** — `zforge run HANDOVER-001 --task TASK-002` (REQ-001, REQ-002, REQ-004)
   - Kiểm tra snapshot mà manifest pin có đúng hash; tạo `RUN-007`; tạo worktree
     trên branch `zforge/TASK-002/RUN-007` từ commit baseline của manifest.
   - Agent nhận nội dung hợp đồng đã chốt của TASK-002, cùng outcome, behavior,
     solution và breakdown đã chốt làm context.
   - Chạy vòng code → verify; verify pass ở vòng 2.
   - Người dùng thấy: tiến độ từng vòng, rồi `RUN-007 verified — branch
     zforge/TASK-002/RUN-007, candidate 1a2b3c…, 2 lượt verify, $0.84`.
2. **Xem kết quả** — `zforge run status RUN-007` (REQ-003, REQ-005)
   - Trạng thái, lịch sử sự kiện, từng lần verify với candidate, trace của từng
     lần gọi agent (như `zforge trace`), chi phí đã dùng so với budget.
   - `.zforge/runs/RUN-007/result.md` được sinh lại, chỉ để đọc.
3. **Knowledge cập nhật** — sau khi RUN-007 `verified` (REQ-011)
   - `zforge knowledge index`: requirement của TASK-002 chuyển thành
     `verified (RUN-007, candidate 1a2b3c…)`.
4. **Liệt kê** — `zforge run list [--handover HANDOVER-001]` (REQ-008)
   - Mỗi dòng: run, task, trạng thái, lý do nếu dừng, chi phí, thời điểm.

### Biên

5. **Snapshot bị sửa** — hash của file trong `.records/revisions/` không khớp manifest (REQ-001)
   - Từ chối trước khi tạo worktree: `contract snapshot 03-solution rev 2 does not
     match HANDOVER-001; nothing was run`. Không có RUN nào được tạo.
6. **Sửa hợp đồng trong lúc chạy** — người dùng hay agent sửa `tasks/TASK-002.md`
   trong lúc RUN-007 đang chạy (REQ-001)
   - RUN-007 vẫn dùng bản đã pin, và cả kết quả lẫn record đều ghi bản đó. Bản
     nháp mới chỉ có hiệu lực qua review, accept và handover mới.
7. **Task có dependency** — `zforge run HANDOVER-001 --task TASK-003`, với TASK-003 `depends_on: [TASK-002]` (REQ-009)
   - Từ chối: `TASK-003 depends on TASK-002; running dependent tasks comes with
     Mốc C`. Không có RUN nào được tạo.
8. **Chạy lại khi đã verified** — `zforge run HANDOVER-001 --task TASK-002` khi RUN-007 đã `verified`
   - Cho phép, tạo lần chạy mới, nhưng cảnh báo trước rằng đã có lần chạy
     verified. Không ghi đè RUN-007.

### Lỗi

9. **Hết vòng thử** — verify vẫn fail sau 3 vòng (REQ-004)
   - RUN-007 `failed`, lý do `verifier budget exhausted`, kèm tên các test fail.
     Worktree và branch được giữ.
10. **Hết budget** — tổng chi phí của TASK-002 trong HANDOVER-001 chạm $3.00 trước khi pass (REQ-006)
    - Mỗi lần gọi agent chỉ được dùng phần budget còn lại của task trong handover
      đó. Chạm giới hạn thì RUN dừng ở `blocked`, lý do `budget: $3.00 of $3.00
      used`, không gọi agent nữa.
    - `run retry` hoặc `zforge run` lại cùng task trong HANDOVER-001 bị từ chối:
      `budget of TASK-002 in HANDOVER-001 is used up; hand over again to continue`.
11. **Cần sửa hợp đồng** — agent kết luận không thể giữ AC-02 nếu không đổi response shape (REQ-010)
    - Agent ghi `changes/CHANGE-001.md` theo mẫu §8 và dừng. RUN `blocked`, lý do
      `amendment: CHANGE-001`.
    - Người dùng review CHANGE-001. Nếu đồng ý thì sửa file hợp đồng qua review,
      accept, rồi handover mới và chạy lại. Nếu không thì hủy RUN.
12. **Worktree không tạo được** — branch đã tồn tại, đĩa đầy, không phải git repo (REQ-002)
    - RUN ghi `failed` với lý do cụ thể trước khi gọi agent; không có chi phí.

### Hồi phục

13. **Bị ngắt** — process của lần chạy chết giữa chừng (REQ-007)
    - Lần đọc trạng thái kế tiếp (`run status`, `run list`) ghi `failed`, lý do
      `interrupted`; worktree được giữ.
    - `zforge run retry RUN-007` tạo `RUN-008`, pin cùng manifest, trên branch
      mới, bắt đầu từ commit baseline. Record của RUN-008 ghi rằng nó chạy lại
      RUN-007. RUN-008 dùng phần budget còn lại sau RUN-007 (REQ-006).
14. **Hủy** — `zforge run cancel RUN-007` (REQ-008)
    - Dừng agent và mọi process test của lần chạy (SIGTERM, grace, SIGKILL), ghi
      `cancelled`. Worktree được giữ.
15. **Crash trong lúc ghi** — process chết giữa lúc ghi kết quả verify (REQ-003)
    - Sự kiện chưa ghi vào nhật ký thì coi như bước đó chưa xảy ra; view được
      sinh lại. Không bao giờ có `verified` mà thiếu evidence.

## Quy tắc nghiệp vụ

- Trạng thái của lần chạy đi theo §6.1: `ready → running → verifying → verified`;
  `running/verifying → blocked | failed | cancelled`; `verifying → running` khi
  sửa trong hợp đồng. `failed` và `cancelled` là kết thúc; chạy lại là lần chạy mới.
- `blocked` là trạng thái dừng của một lần chạy (budget, amendment). Tiếp tục
  sau `blocked` cũng là một lần chạy mới, vì hợp đồng hoặc budget đã khác.
- Chỉ runtime ghi nhật ký sự kiện; lời báo cáo của agent không làm đổi trạng thái.
- Branch của lần chạy không bao giờ được push hay merge bởi zforge.
- **Budget là tổng cho mọi lần chạy của một task trong một handover.** Chi phí
  của mọi RUN (kể cả failed, cancelled, interrupted) của cùng task trong cùng
  manifest được cộng lại; phần còn lại là trần cho lần chạy kế tiếp. Hết budget
  thì cần một handover mới (tức là người dùng quyết định chi thêm).
- Mặc định `zforge run` chạy foreground: tiến độ in ra terminal, Ctrl-C ngắt lần
  chạy (ghi `cancelled`). `--async` chạy nền qua job.

## Hành vi cũ cần bảo toàn

- `zforge ship`, `verify`, `code`, `job *`, MCP `ship`/`verify` giữ nguyên, với
  mọi flow v1 (Full, Fixbug, Spike, Docs).
- `zforge trace <ID>` vẫn đọc được trace của task v1.
- Handover manifest đã ghi không bị sửa.

## Câu hỏi còn mở

- [x] Budget cho từng lần chạy hay tổng? — **tổng cho mọi lần chạy của task trong handover** (ghi ở Quy tắc nghiệp vụ)
