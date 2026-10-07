---
name: Audit finding (maintainers)
about: Publish a defect triaged from the private Loupe audit repositories
title: "`<crate>`: <what the fix guarantees>"
labels: ""
assignees: ""
---

<!--
For SRI maintainers. The triage rules (placement, labels, closure) are in AGENTS.md, under
"Triaging audit findings". Delete these comments before submitting.
-->

## Affected crates and implementations

<!--
Every crate and implementation checked for this defect. In `channels_sv2` that means the
server/client x standard/extended twins. List the unaffected ones too, each with one line on why.
-->

## Problem

<!-- What goes wrong, where, and under which conditions. Pin code links to a commit SHA. -->

## Impact

<!-- Who is affected, what it costs them, and what a peer or attacker needs to trigger it. -->

## Expected outcome

<!--
The guarantees that must hold once this is fixed, observable from outside the code.
A PR is reviewed against these.
-->

## Possible approach (non-binding)

<!-- Implementation and test ideas. A PR may solve the problem another way. -->

## Loupe findings

<!-- Every Loupe finding reporting this defect. They close together with this issue. -->

- [ ] project-loupe/audit-stratum#N: one-line summary

## Related issues and PRs

<!-- Facts only, e.g. "same lifecycle as #2330", or the sv2-apps issue a companion PR will close. -->
