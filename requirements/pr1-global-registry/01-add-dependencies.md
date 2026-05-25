# PR1-01 — Add Cargo dependencies

## Goal

Bring in the crates needed by the global registry layer.

## Files touched

- `Cargo.toml`

## Implementation

Add to `[dependencies]`:

```toml
fs2 = "0.4"
chrono = { version = "0.4", features = ["serde"] }
regex = "1"
```

Existing crates we will reuse (already present): `serde`, `serde_yaml`, `serde_json`, `anyhow`, `thiserror`, `dirs`.

## Acceptance criteria

- [ ] `cargo build` succeeds after adding deps.
- [ ] `cargo tree | grep -E "^(fs2|chrono|regex) "` shows each at expected versions.
- [ ] No new transitive duplicate warnings introduced (`cargo tree -d` diff is empty for new entries).

## Depends on

None.

## Notes

- Pin minor versions only (no exact pins) — these are stable crates.
- If `regex` is later pulled by a transitive dep, keep the explicit entry for documentation purposes.

---

```yaml
status: done
owner: ""
pr: ""
```
