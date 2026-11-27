# Agentic development workflow (Claude Code)

Development tooling only; not part of the product. It changes no code,
dependencies or architecture of Framelet.

## Agents (`.claude/agents/`)

| Agent | Model | Effort | Purpose |
|---|---|---|---|
| `scout` | haiku | low | Simple, mechanical tasks |
| `reviewer` | sonnet | low | Read-only diff review |
| `implementer` | opus | low | Default for non-trivial work and plans |
| `architect` | opus | high | Escalation; structural and security planning |

`opus`, `sonnet` and `haiku` are aliases for the current model of that family.
To pin a version, set `model:` to an exact model ID instead.

## Flow (`/deliver <task>`)

Triage → plan → implement → review → apply feedback → required checks.
An agent that cannot finish replies `ESCALATE: ...` and the orchestrator
hands the task to the next tier (`scout` → `implementer` → `architect`).
After three review rounds without approval the task goes to `architect`. If
that also fails, the workflow stops and reports to the user.

## Limits

Escalation is a prompt convention the orchestrator evaluates, not a built-in
Claude Code mechanism. Its reliability depends on agents following it. The
required checks in AGENTS.md stay the quality gate.
