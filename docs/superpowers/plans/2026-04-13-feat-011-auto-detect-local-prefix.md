# FEAT-011 Auto-Detect Local Prefix Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detectar `local_prefix` automaticamente em runtime quando ele estiver ausente no config.

**Architecture:** O walker passa a expor uma heurística pura para escolher `src`, `app` ou vazio com base na distribuição de arquivos-fonte por diretório raiz. O CLI calcula um `effective_local_prefix` antes do discovery e reaproveita esse valor em cache, warnings e geração de nomes.

**Tech Stack:** Rust, clap CLI, testes unitários e integração com `cargo test`

---

## Chunk 1: Walker Heuristic

### Task 1: Adicionar testes unitários da heurística

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs`

- [ ] **Step 1: Escrever testes falhando para detecção automática**
- [ ] **Step 2: Rodar os testes do walker e confirmar falha**
- [ ] **Step 3: Implementar a heurística mínima**
- [ ] **Step 4: Rodar os testes do walker e confirmar sucesso**

### Task 2: Expor função reutilizável

**Files:**
- Modify: `crates/graphify-extract/src/walker.rs`
- Modify: `crates/graphify-extract/src/lib.rs`

- [ ] **Step 1: Expor função pública para detectar prefixo**
- [ ] **Step 2: Garantir reaproveitamento dos mesmos filtros de linguagem/exclusão**
- [ ] **Step 3: Rodar testes do pacote `graphify-extract`**

## Chunk 2: CLI Integration

### Task 3: Integrar detecção em `run_extract()`

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Escrever teste de integração para projeto sem `local_prefix`**
- [ ] **Step 2: Rodar o teste e confirmar falha**
- [ ] **Step 3: Calcular `effective_local_prefix` em runtime**
- [ ] **Step 4: Aplicar valor efetivo em discovery, warnings e cache**
- [ ] **Step 5: Logar detecção automática**
- [ ] **Step 6: Rodar o teste e confirmar sucesso**

## Chunk 3: Verification and Tracking

### Task 4: Verificação final

**Files:**
- Modify: `docs/TaskNotes/Tasks/FEAT-011-auto-detect-local-prefix.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Rodar `cargo test -p graphify-extract`**
- [ ] **Step 2: Rodar `cargo build -p graphify-cli --bin graphify`**
- [ ] **Step 3: Rodar `cargo test --test integration_test`**
- [ ] **Step 4: Atualizar TaskNotes e sprint**

Plan complete and saved to `docs/superpowers/plans/2026-04-13-feat-011-auto-detect-local-prefix.md`. Ready to execute?
