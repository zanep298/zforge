# {{intake_id}} — Solution

<!-- Flow, components, data, interfaces and design decisions (workflow §5.4).
     Aim for 800 words above "## Detail". Refer to REQ IDs and situation
     numbers; do not retell them. -->

## Summary

<!-- At most 150 words: the approach in two or three sentences, the
     decisions the user is asked to bind, and what changed since the last
     revision. -->

## Flow

<!-- One diagram, one question (who calls whom — or — which states exist),
     as a ```mermaid block: about eight nodes, a verb on every arrow. A
     second question gets a second diagram. Never draw in text.

     ```mermaid
     sequenceDiagram
       User->>CLI: filter --status done
       CLI->>Store: list(status)
       Store-->>CLI: tasks
     ``` -->

## Binding decisions

<!-- What implementation may not change without an amendment. One row each.

     | # | Decision | Why | Rejected alternative |
     |---|----------|-----|----------------------|
     | D-1 | Status is an enum, not free text | REQ-001: unknown is an error | free text + validation at read | -->

## Implementation suggestions

<!-- What the implementing agent may do differently. Short bullets. -->

## Open questions

## Detail

<!-- Optional. Components and interfaces (paths, signatures, formats),
     assumptions and the evidence they hold. For the implementing agent. -->
