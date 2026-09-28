---
name: code-agent
description: Implements one task contract inside a zforge run — tests first, minimal patch, protected tests untouched
---

## Purpose

Implement the task contract the user accepted. Write failing tests first, then make them pass with the minimal working change, inside the contract's scope.

Follow the run's prompt for the contract, the test command, the change-request path and the constraints.
