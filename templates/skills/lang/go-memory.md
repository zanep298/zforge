# Skill: go-memory

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Keep memory bounded and allocations low while writing Go backend code: request
handlers, workers, consumers, anything that runs per request or per message.
Apply it as you write, not as a later pass. For a measured memory problem, use
`zforge-performance-optimize` (`performance-optimize.md` in the skills store)
instead: profile first, change one thing, measure again.

## Bound Everything From Outside

Memory a client or upstream controls must have a limit.

- Request bodies: wrap with `http.MaxBytesReader` before decoding
- Never `io.ReadAll` on a body, file or response of unknown size — stream it
  (`json.NewDecoder`, `io.Copy`, `bufio.Scanner` with `Buffer` set) or limit it
  with `io.LimitReader`
- Queries: always `LIMIT` / paginate; never load a whole table into a slice
- Iterate `*sql.Rows` and process row by row; always `defer rows.Close()` and
  check `rows.Err()`
- In-memory caches and maps keyed by input need a size cap and eviction (LRU,
  TTL) — an unbounded `map` is a leak
- Channels: bounded buffer; a producer that can outrun its consumer needs
  backpressure, not a bigger buffer

```go
r.Body = http.MaxBytesReader(w, r.Body, 1<<20) // 1 MiB
var req CreateOrderRequest
if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
    return fmt.Errorf("decode request: %w", err)
}
```

## Goroutines Must End

A goroutine that never returns keeps everything it references alive.

- Every goroutine has a way to stop: `ctx.Done()`, a closed channel, or a
  finished input
- A send or receive that may block forever needs a `select` with `ctx.Done()`
- Bound concurrency for fan-out (`errgroup.Group.SetLimit`, a worker pool,
  a semaphore) — never one goroutine per item of an unbounded input
- `time.Ticker`: `defer t.Stop()`; no `time.After` inside a hot loop (each call
  allocates a timer) — reuse a `time.Timer`

```go
g, ctx := errgroup.WithContext(ctx)
g.SetLimit(8)
for _, id := range ids {
    g.Go(func() error { return s.process(ctx, id) })
}
return g.Wait()
```

## Close What You Open

- `defer resp.Body.Close()` on every `http.Response`, after checking the error;
  drain it (`io.Copy(io.Discard, resp.Body)`) when you stop reading early so the
  connection is reused
- Files, `*sql.Rows`, `*sql.Stmt`, gRPC streams, Kafka/Redis subscriptions:
  close on every path, including errors
- `context.WithCancel` / `WithTimeout`: always `defer cancel()`

## Allocate Less on Hot Paths

- Preallocate when the size is known: `make([]T, 0, n)`, `make(map[K]V, n)`
- Build strings with `strings.Builder` (`Grow` when known) or `bytes.Buffer`,
  not `+=` in a loop
- Use `strconv` over `fmt.Sprintf` for single conversions
- Avoid `[]byte` ↔ `string` conversions in loops; work in one type
- Pass large structs by pointer; keep small values (a few words) by value —
  pointers to short-lived values push them to the heap
- Avoid boxing into `any` / interfaces on hot paths; it allocates
- `sync.Pool` only for frequently allocated, short-lived buffers on a measured
  hot path; `Reset` before `Put`; never pool objects that grow without bound
  (drop buffers above a cap instead of returning them)

```go
buf := bufPool.Get().(*bytes.Buffer)
buf.Reset()
defer func() {
    if buf.Cap() <= 64<<10 {
        bufPool.Put(buf)
    }
}()
```

## Don't Pin Large Memory

- A sub-slice keeps its whole backing array alive: copy out what you keep
  (`slices.Clone`, `bytes.Clone`, `strings.Clone`) from a large buffer
- Deleting from a map does not shrink it; rebuild a map that grew and emptied
- Nil out pointers in a slice you shrink (`s[i] = nil` before reslicing) or
  use `slices.Delete`, which clears the tail
- Closures capture variables, not values: a long-lived closure capturing a
  large struct keeps it alive

## Runtime Settings

- In containers set `GOMEMLIMIT` (about 80–90% of the memory limit) so the GC
  works harder before the OOM killer acts; leave `GOGC` at default unless
  measured
- Expose `net/http/pprof` on an internal-only port (never the public listener)
  so heap and goroutine profiles can be taken in production

## Verify

- Allocation-sensitive code gets a benchmark with `b.ReportAllocs()`; compare
  with `benchstat`, never a single run
- Check escapes when it matters: `go build -gcflags='-m' ./pkg/...`
- Leaks: `go test -run X -memprofile mem.out`, `go tool pprof -sample_index=inuse_space`;
  for goroutines, `go.uber.org/goleak` in tests where the project already uses it
- Run the race detector (`go test -race`) on code with new goroutines

## Do Not Do

- No `io.ReadAll` on input whose size you do not control
- No goroutine without a stop condition
- No unbounded map, slice, channel or cache fed by requests
- No `sync.Pool`, `unsafe` or manual tuning without a benchmark showing the gain
- No global mutable buffers shared across requests
