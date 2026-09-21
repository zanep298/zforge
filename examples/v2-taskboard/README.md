# Taskboard — project tham chiếu cho zForge v2

API quản lý công việc nhỏ, dùng dữ liệu giả lập. Đây là **project để zForge xử lý**,
không phải implementation của engine v2. Không có deployment, production, tài
khoản thật hoặc credentials. Không sử dụng cho ứng dụng production.

## Có gì sẵn?

- Python standard library, không cần cài dependency ứng dụng.
- Tạo, đọc và liệt kê task; trạng thái `open | done`.
- Dữ liệu trong bộ nhớ, tách theo server process, restart là reset.
- Unit/HTTP tests và E2E từ container kiểm thử sang container API.
- Docker image pin theo digest, chạy non-root/read-only, không publish host port,
  không mount repo/credentials/Docker socket và dùng network nội bộ.
- [Product context và API contract](./docs/product.md),
  [test-case checklist](./docs/test-cases.md), [task mẫu](./tasks/README.md).

Không có DB của ứng dụng để giảm setup. Điều này độc lập với quyết định dùng
Markdown/YAML để lưu state của zForge.

## Chạy baseline bằng Docker trên macOS

Cần Docker đang chạy, Compose có sẵn và host Python 3.9+ để gọi script điều phối.
Ứng dụng/tests chạy bằng Python 3.12 trong Docker; không chạy code agent trên host.

Từ thư mục này:

```sh
python3 scripts/check.py
```

Script tạo Compose project name riêng, build image, chờ API healthy, chạy
unit/HTTP tests rồi E2E. Exit code khác 0 nếu tests hoặc cleanup thất bại.
Timeout toàn lần chạy là 300 giây, cleanup tối đa 60 giây. `finally` dọn đúng
container/network/volume thuộc lần chạy đó; Docker image/build cache được giữ lại.
Lần đầu cần mạng để tải base image. Runtime không cần Internet.

Hai terminal có thể chạy lệnh trên đồng thời: không cố định container name,
host port, volume hoặc mutable data. Đây chỉ kiểm tra isolation của **môi trường
mẫu**, chưa chứng minh scheduler zForge chạy song song đúng.

Nếu host/process bị kill cứng, `finally` không thể bảo đảm cleanup. Dùng đúng
project name `zforge-tb-...` được in khi bắt đầu để kiểm tra và dọn:

```sh
docker compose --project-name <printed-project-name> --file compose.yaml down --volumes --remove-orphans
```

Không dùng `docker system prune` hay dọn project khác.

## Cấu trúc

```text
taskboard/           API server, store và feature modules
tests/               Unit tests và HTTP tests trên server riêng mỗi test
checks/              HTTP contract dùng chung, reviewer có thể đọc
e2e/                 Chạy contract qua HTTP sang container API
tasks/               Bài tập chưa implement: task đơn, batch, nhiều phase
docs/                Context sản phẩm và checklist cho reviewer
scripts/check.py     Điều phối môi trường Docker dùng một lần
```

## Dùng để đánh giá zForge như thế nào?

1. Giữ một bản baseline sạch của thư mục này; khi thử agent, copy vào repository
   Git tạm riêng, không giao cả checkout zForge làm project đích. Người điều phối
   ghi nhận base commit/tree hash; fixture không tự tạo repo hoặc commit.
2. Cung cấp product context và một task trong `tasks/` qua intake đã hỗ trợ.
   Đây là brief Markdown, **không phải native-v2 task record** và không giả định
   CLI/config schema v2 đã implement.
3. Agent bổ sung feature và tests theo acceptance criteria. Baseline chỉ xanh
   chưa đủ: phải chứng minh test mới phát hiện thiếu feature ở baseline.
4. Chạy Docker checks trên tree kết quả; reviewer kiểm tra expected/actual,
   test code liên quan và các giới hạn trong evidence package.
5. Khi engine v2 có sẵn, nối thêm fake/real agent, model routing, recovery,
   evidence gates và concurrency evaluation bên ngoài project này.

Tests ở đây là public và có thể chỉnh sửa, không phải independent/hidden
evaluator. Engine/controller sau này phải bảo vệ quyền quyết định completion,
ghi tree/image hashes, log/exit code và kiểm tra test không bị xoá hoặc vô hiệu.
Compose mẫu cũng không thay thế conformance của Docker agent sandbox ADR-003.
