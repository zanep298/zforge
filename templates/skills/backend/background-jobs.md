# Skill: backend-background-jobs

> General checklist. The project's own conventions — its accepted `conventions.md`
> and `rules.md` (a run's prompt shows the ones that apply) — and the nearest
> existing code in the module you change override it. Follow it only where the
> project is silent; never restructure code to match it.

## Purpose
Design and implement reliable background jobs, queues, schedulers, and async
workers. Use this before touching retry loops, cron jobs, or message consumers.

## Checklist

- [ ] Job inputs are minimal, serializable, and version-tolerant
- [ ] Work is idempotent or protected by dedupe keys
- [ ] Retry policy distinguishes transient vs permanent failures
- [ ] Dead-letter or failure visibility exists for exhausted retries
- [ ] Concurrency limits and locking behavior are explicit
- [ ] Tests cover retry, duplicate delivery, and partial failure
- [ ] Shutdown/cancellation behavior does not corrupt in-flight work

## Do Not Do

- Do not assume exactly-once delivery
- Do not hide failed jobs without operator visibility
- Do not enqueue jobs before the related transaction is safely committed
