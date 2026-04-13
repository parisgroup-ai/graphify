# FEAT-004 CI Quality Gates Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar `graphify check` com gates de ciclos e hotspot máximo para uso em CI.

**Architecture:** O CLI ganhará um subcomando `Check` que reaproveita `run_extract()` e `run_analyze()` para produzir um resumo em memória por projeto. Um helper puro avaliará limites opcionais, acumulará violações e alimentará tanto a saída humana quanto a saída JSON, com exit code `1` quando qualquer projeto falhar.

**Tech Stack:** Rust, clap CLI, serde JSON, testes unitários e integração com `cargo test`

---

## Chunk 1: Gate Contract

### Task 1: Cobrir a avaliação de gates por unidade

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`

- [ ] **Step 1: Escrever testes unitários falhando para avaliação sem limites, hotspot máximo e múltiplas violações**
- [ ] **Step 2: Rodar os testes unitários filtrados e confirmar falha**
- [ ] **Step 3: Implementar os tipos e helpers mínimos de avaliação**
- [ ] **Step 4: Rodar os testes unitários novamente e confirmar sucesso**

## Chunk 2: CLI Check Command

### Task 2: Adicionar o subcomando `check`

**Files:**
- Modify: `crates/graphify-cli/src/main.rs`
- Modify: `tests/integration_test.rs`

- [ ] **Step 1: Escrever testes de integração falhando para sucesso, falha por ciclos, JSON e multi-projeto**
- [ ] **Step 2: Rodar os testes de integração filtrados e confirmar falha**
- [ ] **Step 3: Adicionar a variante `Commands::Check` e fazer o parsing das flags**
- [ ] **Step 4: Implementar o fluxo em memória usando `run_extract()` e `run_analyze()`**
- [ ] **Step 5: Implementar saída humana, saída JSON e exit code**
- [ ] **Step 6: Rodar os testes de integração filtrados e confirmar sucesso**

## Chunk 3: Verification and Tracking

### Task 3: Validar e sincronizar tracking

**Files:**
- Modify: `docs/TaskNotes/Tasks/FEAT-004-ci-quality-gates.md`
- Modify: `docs/TaskNotes/Tasks/sprint.md`

- [ ] **Step 1: Rodar `cargo test --test integration_test`**
- [ ] **Step 2: Rodar `cargo build -p graphify-cli --bin graphify`**
- [ ] **Step 3: Rodar `cargo test -p graphify-cli`**
- [ ] **Step 4: Atualizar TaskNotes e sprint após verificação**

Plan complete and saved to `docs/superpowers/plans/2026-04-13-feat-004-ci-quality-gates.md`. Ready to execute?
