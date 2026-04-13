# FEAT-011 Auto-Detect Local Prefix - Design

**Task:** [[FEAT-011-auto-detect-local-prefix]]
**Status:** Approved
**Date:** 2026-04-13

## Goal

Quando `local_prefix` estiver ausente no `graphify.toml`, o Graphify deve detectar um prefixo efetivo em runtime para melhorar a descoberta de arquivos e a estabilidade dos IDs de módulo, sem persistir alterações de volta no arquivo de configuração.

## Decisions

- `local_prefix` explícito no config continua soberano.
- A detecção será feita em runtime, antes de `discover_files()`.
- A heurística será conservadora:
  - usar `src` se `src` concentrar mais de 60% dos arquivos-fonte elegíveis
  - senão usar `app` se `app` concentrar mais de 60% dos arquivos-fonte elegíveis
  - em qualquer outro caso usar prefixo vazio
- Arquivos-fonte na raiz pesam a favor do prefixo vazio.
- O valor efetivo detectado será usado em descoberta, warnings e cache.
- O CLI deve logar quando a detecção automática for usada.

## Implementation Shape

### Walker

Adicionar uma função isolada em `crates/graphify-extract/src/walker.rs` para detectar o prefixo efetivo com a mesma noção de linguagens e exclusões já usada pelo discovery.

Responsabilidades:
- contar arquivos-fonte elegíveis por diretório raiz
- considerar `src` e `app` como candidatos preferenciais
- retornar `""` quando não houver dominância clara

### CLI

Em `crates/graphify-cli/src/main.rs`:
- calcular `effective_local_prefix` em `run_extract()`
- usar o valor efetivo em `discover_files()`
- usar o mesmo valor em warnings e no `ExtractionCache`
- emitir log apenas quando a detecção automática acontecer

## Testing

- testes unitários em `walker.rs` para `src`, `app`, empate/ambiguidade e arquivos na raiz
- teste de integração cobrindo execução sem `local_prefix`
- manter regressões recentes do pipeline passando

## Non-Goals

- persistir sugestão no `graphify.toml`
- inferir prefixo via `tsconfig`
- heurísticas por framework além de `src`/`app`
