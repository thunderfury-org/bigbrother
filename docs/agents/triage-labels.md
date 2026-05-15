# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role, use the corresponding label string from this table.

## Readiness criteria

Use `ready-for-agent` only when the requirement is specific enough for implementation without guessing. At minimum, it should define:

- the problem and desired outcome
- in-scope work and non-goals
- acceptance criteria in behavior terms
- affected entrypoints, modules, or surfaces when known
- relevant dependencies, prerequisites, config, or rollout constraints
- a clear verification method

If key information is missing, or unresolved decisions would materially change the implementation approach, use `needs-info` instead of `ready-for-agent`.
