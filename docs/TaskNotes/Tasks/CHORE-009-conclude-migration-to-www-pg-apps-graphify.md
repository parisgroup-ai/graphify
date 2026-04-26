---
uid: chore-009
status: done
priority: low
scheduled: 2026-04-26
completed: 2026-04-26
pomodoros: 0
projects:
- '[[sprint.md|Current Sprint]]'
tags:
- task
- chore
ai:
  parallelParts: 0
  needsReview: false
  uncertainty: low
  hintsInferred: true
---

# Conclude migration to ~/www/pg/apps/graphify

Done-record retroativo: a migração `~/ai/graphify` → `~/www/pg/apps/graphify` foi concluída e auditada na sessão de 2026-04-26. Esta task existe apenas como registro auditável; nenhuma ação adicional é necessária.

## Description

Em 2026-04-26 o repositório foi movido de `~/ai/graphify` para `~/www/pg/apps/graphify`. A sessão de fechamento confirmou que working tree, binário instalado (`v0.13.1`), shell rc files, cron, launchd, configs do Claude/Codex e arquivos `.code-workspace` referenciam apenas o caminho novo. Cache residual `~/.claude/security_warnings_state_*.json` referenciando o caminho antigo foi removido. `.claude/session-context-gf.json` foi commitado como baseline pós-migração e marcado com `git update-index --skip-worktree` para suprimir churn local.

## Checklist

- [x] Working tree no caminho novo
- [x] Binário `~/.cargo/bin/graphify` rebuildado (`v0.13.1`)
- [x] Shell rc files sem referências ao caminho antigo
- [x] Cron / launchd sem referências ao caminho antigo
- [x] Configs do Claude / Codex apontando pro caminho novo
- [x] `.code-workspace` files atualizados
- [x] Cache `~/.claude/security_warnings_state_*.json` referenciando caminho antigo removido
- [x] `.claude/session-context-gf.json` baseline commitado + `skip-worktree` aplicado
- [x] Tag/release `v0.13.1` no GitHub (publicado em 2026-04-26 14:00 UTC)
- [ ] Coordenação com sibling instances (deferred — sem evidência de sibling ativo no momento do fechamento)

## Notes

- Reverter o `skip-worktree` se o brief precisar entrar em git status novamente: `git update-index --no-skip-worktree .claude/session-context-gf.json`
- Audit completo está documentado em `.claude/session-brief.md` (close de 2026-04-26)

## Related

- [[sprint]] - Current sprint
- [[activeContext]] - Active context
