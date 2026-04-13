# FEAT-004 CI Quality Gates - Design

**Task:** [[FEAT-004-ci-quality-gates]]
**Status:** Approved
**Date:** 2026-04-13

## Goal

Adicionar o subcomando `graphify check` para validar gates arquiteturais em CI, com saída humana ou JSON e exit code não-zero quando qualquer projeto violar os limites configurados.

## Decisions

- O comando será `graphify check`.
- O `check` recalcula a análise em memória a partir do `graphify.toml`.
- O comando aceitará:
  - `--config <path>`
  - `--max-cycles <usize>`
  - `--max-hotspot-score <f64>`
  - `--json`
  - `--project <name>`
  - `--force`
- Em configs multi-projeto, o comando falha se qualquer projeto violar qualquer gate.
- Sem `--json`, a saída será legível para humanos.
- Com `--json`, a saída será estável e adequada para CI parsers.
- Sem limites explícitos, o comando imprime apenas o resumo e sai com código `0`.

## Implementation Shape

### CLI

Adicionar uma variante `Check` em `Commands` dentro de `crates/graphify-cli/src/main.rs` e conectar o branch correspondente no `match` principal.

O subcomando:
- carrega o config
- filtra projetos opcionalmente com `--project`
- roda `run_extract()` e `run_analyze()` em memória
- avalia violações por projeto
- imprime saída humana ou JSON
- encerra com `std::process::exit(1)` quando houver violações

### Gate Evaluation

Criar helpers isolados no mesmo arquivo do CLI para manter o contrato testável:
- um resumo por projeto com nós, arestas, comunidades, ciclos e hotspot máximo
- uma lista de violações por projeto
- um agregado final com `ok`, total de violações e lista de projetos

Os gates iniciais serão:
- `max_cycles`
- `max_hotspot_score`

### Output Contract

Saída humana:
- uma linha por projeto com status `PASS` ou `FAIL`
- resumo final informando se todos os checks passaram ou quantas violações ocorreram

Saída JSON:
- objeto raiz com `ok`, `violations` e `projects`
- cada projeto com `name`, `ok`, `summary`, `limits` e `violations`

## Testing

- testes unitários para o helper de avaliação:
  - sem limites
  - hotspot máximo correto
  - múltiplas violações acumuladas
- testes de integração para:
  - falha com `--max-cycles 0`
  - sucesso com limites permissivos
  - payload JSON estável
  - falha multi-projeto quando apenas um projeto viola

## Non-Goals

- ler `analysis.json` existente
- escrever arquivos extras de output para `check`
- implementar wrapper de GitHub Action
- adicionar gates além de ciclos e hotspot máximo nesta versão
