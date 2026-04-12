# Graphify — Scope Document v1

## Repositório

- **Nome:** cursos (monorepo ToStudy)
- **Caminho local:** `~/www/cursos`
- **Linguagens:** TypeScript/TSX (6.162 arquivos), Python (4.899 arquivos)
- **Estrutura:** monorepo com 8 apps e 19 packages

## Baseline

- **Tag:** `graphify-baseline-v1`
- **Commit:** `4162827d75307f2fba563089ee456f8b84752a49`
- **Data:** 2026-04-11 22:21:01 -0300
- **Mensagem:** `chore(pageshell): upgrade @parisgroup-ai/pageshell 4.18.2 → 4.20.1`
- **Checkout:** `git checkout graphify-baseline-v1`

## Escopo

### O que será analisado

- **Apps (8):** ana-service, api, mcp-server, mobile, pageshell-core, tostudy-cli, video-engine, web
- **Packages (19):** analytics, api, config, course-builder, database, email, env, jobs, llm-costs, logger, mcp-cli-core, mcp-setup, resilience, task-cli, test-utils, tostudy-core, types, typesense, validators
- **Tipos de análise:**
  - Dependências entre apps e packages (imports/exports)
  - Funções duplicadas entre módulos (lógica equivalente em locais distintos)
  - Fluxos duplicados entre apps (sequências de operações repetidas com implementações diferentes)
  - Código morto (exports sem consumers, funções sem chamadas externas)

### Justificativa

Desenvolvedor solo com LLM em fase pré-lançamento. Cada sessão de LLM opera sem memória das anteriores, gerando risco de duplicação de funções, fluxos inconsistentes e código órfão acumulado ao longo do desenvolvimento. Análise completa (apps + packages) é necessária para garantir consistência antes do release.

## Fora do Escopo

| Excluído | Motivo |
|---|---|
| `node_modules/` | Dependências externas, não código do projeto |
| `dist/`, `.next/`, `build/` | Artefatos de build gerados automaticamente |
| `__pycache__/` | Cache do Python |
| `tests/`, `__tests__/`, `*.test.*`, `*.spec.*` | Testes não representam arquitetura de produção |
| `migrations/` | SQL gerado, não lógica de negócio |
| `docs/`, `*.md` (exceto este) | Documentação, não código |
| `.env*`, `credentials*` | Segredos — nunca analisados |
| `patches/`, `docker/` | Infraestrutura, fora do foco de arquitetura de código |

## Critérios de Sucesso

| # | Critério | Verificação |
|---|---|---|
| 1 | Grafo com todos os apps e packages como nós | `graph.json` contém ≥ 27 nós (8 apps + 19 packages) com arestas de dependência |
| 2 | Funções duplicadas identificadas | `duplicated-functions.md` lista pares de funções com lógica equivalente, caminho dos arquivos e sugestão de consolidação |
| 3 | Fluxos duplicados identificados | `duplicated-flows.md` lista sequências de operações repetidas entre apps/packages com evidência de código |
| 4 | Código morto detectado | `dead-code.md` lista exports sem nenhum import em outros módulos |
| 5 | Relatório navegável gerado | Arquivo HTML abre no browser sem erros |

## Checklist de Ambiente

### Runtimes

| Dependência | Versão | Verificação |
|---|---|---|
| Node.js | v24.7.0 | `node --version` |
| Python | 3.12.5 | `python3 --version` |
| pnpm | 10.7.1 | `pnpm --version` |

### Bibliotecas Python (Graphify)

| Pacote | Propósito | Instalação |
|---|---|---|
| `networkx` | Construção e análise do grafo | `pip install networkx` |
| `tree-sitter` | Parsing de AST para TypeScript e Python | `pip install tree-sitter` |
| `click` | CLI do Graphify | `pip install click` |
| `jinja2` | Geração do relatório HTML | `pip install jinja2` |

### Segurança

- Tokens de acesso: não necessários (análise local, repo já clonado)
- `.env`: nunca commitado — verificar com `git ls-files | grep -i env`
- `.gitignore`: deve cobrir `chroma_db/`, `*.env`, `credentials*`

### Verificação rápida (< 5 min)

```bash
# 1. Clonar e posicionar no baseline
cd ~/www/cursos
git checkout graphify-baseline-v1

# 2. Verificar runtimes
node --version    # esperado: v24.7.0
python3 --version # esperado: 3.12.5
pnpm --version    # esperado: 10.7.1

# 3. Instalar dependências do Graphify
pip install networkx tree-sitter click jinja2

# 4. Verificar instalação
python3 -c "import networkx, click, jinja2; print('OK')"
```

## Riscos e Limitações

| # | Risco | Evidência | Mitigação |
|---|---|---|---|
| 1 | Imports dinâmicos em TypeScript (`await import()`) não são detectados por análise estática | Padrão comum em Next.js (`apps/web`) para lazy loading | Registrar imports dinâmicos separadamente com regex e flag manual |
| 2 | Python `ana-service` usa padrões de metaprogramação (decorators, registros dinâmicos) | FastAPI usa `@router.get()` que registra rotas dinamicamente | Parsear decorators como dependências explícitas |
| 3 | Monorepo com duas linguagens gera grafo heterogêneo | 6.162 TS + 4.899 PY = universos de análise distintos | Gerar subgrafos por linguagem e um grafo unificado de dependência entre apps/packages |

## Ponto de Falha Mais Provável

A instalação do `tree-sitter` com gramáticas para TypeScript e Python. O pacote base instala sem problemas, mas as gramáticas de linguagem (`tree-sitter-typescript`, `tree-sitter-python`) exigem compilação nativa e podem falhar em ambientes sem toolchain C/C++ instalada. Mitigação: documentar `xcode-select --install` (macOS) como pré-requisito e testar a instalação com `python3 -c "import tree_sitter"` antes de prosseguir.
