---
name: deliver
description: Automated workflow for a task - triage, plan, implement, review, apply feedback - delegating to cheap subagents for simple work and escalating to stronger models when needed. Use when the user runs /deliver <task>.
---
Run this workflow for the task given in the arguments. Delegate with the
Agent tool to the agents in `.claude/agents/`. Tiers, cheapest first:
`scout` → `implementer` → `architect`.

1. **Triage.** Classify the task:
   - trivial and mechanical → step 3 with `scout`, no plan needed.
   - touches ports, domain model, dependencies, security or crate structure
     → plan with `architect`.
   - otherwise → plan with `implementer`.
2. **Plan.** Ask the chosen agent for a short plan: files, steps, tests,
   docs (arc42/ADR) to update.
3. **Implement.** Give the plan to the tier that fits (at least
   `implementer` if a plan was needed). If an agent replies with
   `ESCALATE: ...`, hand the task plus that message to the next tier up.
4. **Review.** Run `reviewer` on the diff.
5. **Feedback.** If there are BLOCKER or MAJOR findings, send them to the
   agent that implemented the change, then repeat step 4. After three
   review rounds without approval, escalate to `architect`. If `architect`
   cannot resolve them either, stop and report to the user. MINOR findings
   are fixed if cheap, otherwise listed in the report.
6. **Finish.** Run the required checks from AGENTS.md. Report honestly: each
   check as passed, failed (with output) or not configured; which tiers were
   used and any escalations; open MINOR findings.

7. **Deliver.** Once review is approved and the checks are reported, commit
   the change on the session's development branch, push it, and open a pull
   request (mirror the repository's PR template if present). The PR body
   lists the check results, tiers used and open MINOR findings. Never commit
   a red build or unresolved BLOCKER/MAJOR findings; stop and report instead.
