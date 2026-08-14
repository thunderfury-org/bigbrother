# Triage

Use these labels:

| Label | Meaning |
| --- | --- |
| `needs-triage` | Maintainer still needs to evaluate |
| `needs-info` | Blocked on missing information or an unresolved decision |
| `ready-for-agent` | Specified enough to implement without guessing |
| `ready-for-human` | Requires human implementation |
| `wontfix` | Will not be actioned |

`ready-for-agent` requires all of:

- problem and desired outcome
- in-scope work and non-goals
- acceptance criteria in behavior terms
- affected entrypoints, modules, or surfaces when known
- dependencies, prerequisites, config, or rollout constraints when they affect implementation
- no unresolved product or architecture decision that would change the approach
- a stated verification method

Otherwise use `needs-info`.
