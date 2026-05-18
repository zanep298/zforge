# Skill: backend-observability

## Purpose
Add useful logs, metrics, and traces without noisy or sensitive output. Use this
when a task changes request handling, background work, external calls, or failure
paths.

## Checklist

- [ ] Logs include stable event names and correlation/request IDs
- [ ] Metrics have clear units, labels, and bounded cardinality
- [ ] Traces mark external calls, retries, and meaningful internal spans
- [ ] Errors include enough context to debug without exposing secrets or PII
- [ ] Dashboards or alerts are updated when a new critical path is introduced
- [ ] Tests or smoke checks verify instrumentation paths where practical

## Do Not Do

- Do not log tokens, secrets, passwords, or raw PII
- Do not use unbounded labels such as user email, full URL, or free-form error text
- Do not add logs that duplicate every line of a hot loop
