# Graphify — Scope Document: ana-service

## Repositório

- **Nome:** ana-service
- **Caminho local:** `~/www/cursos/apps/ana-service`
- **Descrição:** Serviço de IA para geração de cursos (Course Builder) usando Claude API. Backend FastAPI que orquestra múltiplos providers LLM, templates Jinja2 e pipelines de geração.
- **Linguagem:** Python 3.13+
- **Arquivos Python (sem testes):** 293
- **Templates Jinja2:** 141
- **Framework:** FastAPI + Pydantic + SQLAlchemy + Redis

## Baseline

- **Tag:** `graphify-baseline-v1`
- **Commit:** `4162827d75307f2fba563089ee456f8b84752a49`
- **Data:** 2026-04-11 22:21:01 -0300
- **Checkout:** `git checkout graphify-baseline-v1`

## Escopo

### O que será analisado

| Camada | Pasta | Arquivos .py | Propósito |
|---|---|---|---|
| Routers | `app/routers/` | 29 | Endpoints da API — ponto de entrada de cada feature |
| Services | `app/services/` | 167 | Lógica de negócio — onde vive a complexidade |
| Skills | `app/skills/` | 30 | Orquestrações de alto nível (study, v3, video) |
| LLM | `app/llm/` | 16 | Gateway, providers, fallback, roteamento de modelos |
| Agents | `app/agents/` | 10 | Personas, stages, schemas de agentes |
| Models | `app/models/` | 9 | Schemas Pydantic — contratos de dados |
| Dependencies | `app/dependencies/` | 5 | Injeção de dependência FastAPI |
| Prompts | `app/prompts/` | 4 | Carregamento e gestão de templates |
| Context | `app/context/` | 3 | Carregamento de contexto e knowledge |
| Errors | `app/errors/` | 5 | Tratamento de erros customizado |
| Jobs | `app/jobs/` | 6 | Tarefas assíncronas (Redis Queue) |
| Utils | `app/utils/` | 3 | Utilitários compartilhados |
| Middleware | `app/middleware/` | 3 | Tracing e tracking de LLM |
| Config/Root | `app/*.py` | 5 | main, config, scheduler, monitoring, redis_keys |

### Tipos de análise

- **Dependências internas:** imports entre camadas (quem chama quem: router → service → llm → provider)
- **Funções duplicadas:** lógica equivalente entre `services/v1/`, `services/v3/`, `services/study/` e `services/llm/`
- **Fluxos duplicados:** pipelines de geração que seguem o mesmo padrão (load template → build prompt → call LLM → parse response) implementados separadamente
- **Código morto:** funções/classes definidas mas nunca importadas ou chamadas

### Justificativa

O `ana-service` é o maior app do monorepo em volume de código Python. Desenvolvido solo com LLM ao longo de múltiplas sessões, tem alto risco de duplicação entre as pastas versionadas (`v1` vs `v3`) e entre domínios paralelos (`study`, `spark`, `video`). A presença de 167 arquivos só em `services/` sugere possível fragmentação de responsabilidades.

### Diferenças em relação ao scope do monorepo

O scope do monorepo mapeia dependências *entre* apps e packages (grafo de alto nível). Este scope mapeia dependências *dentro* de um único serviço (grafo de baixo nível). A granularidade muda: em vez de nós representando apps/packages, aqui cada nó é um módulo Python (`app.services.v3.generation`, `app.llm.gateway`, etc.). Isso exige parsing de imports relativos e absolutos dentro do mesmo pacote.

## Fora do Escopo

| Excluído | Motivo |
|---|---|
| `tests/` e `app/agents/tests/` | Testes não representam arquitetura de produção |
| `app/templates/` (141 arquivos Jinja2) | São templates de prompt, não código Python — analisados separadamente se necessário |
| `__pycache__/` | Cache gerado pelo Python |
| `scripts/` | Scripts auxiliares de manutenção |
| `docs/` | Documentação |
| `.env*`, `redis-acl.conf` | Configuração sensível |
| `uv.lock`, `requirements.txt` | Manifesto de dependências externas, não lógica interna |

## Critérios de Sucesso

| # | Critério | Verificação |
|---|---|---|
| 1 | Grafo com todas as camadas como nós | `graph-ana-service.json` contém nós para routers, services, skills, llm, agents, models, dependencies, prompts, context, errors, jobs, utils, middleware |
| 2 | Arestas de dependência entre camadas | O grafo mostra fluxo router → service → llm → provider sem arestas invertidas inesperadas |
| 3 | Funções duplicadas entre v1 e v3 | `duplicated-functions-ana.md` lista funções com lógica equivalente entre `services/v1/` e `services/v3/` |
| 4 | Fluxos duplicados entre domínios | `duplicated-flows-ana.md` identifica pipelines de geração repetidos entre `study/`, `spark/`, `video/` |
| 5 | Código morto detectado | `dead-code-ana.md` lista módulos, classes ou funções sem nenhum import/chamada no restante do app |

## Checklist de Ambiente

### Runtimes

| Dependência | Versão | Verificação |
|---|---|---|
| Python | ≥ 3.13 | `python3 --version` |
| pip/uv | qualquer | `pip --version` ou `uv --version` |

### Bibliotecas do Graphify

| Pacote | Propósito | Instalação |
|---|---|---|
| `networkx` | Construção e análise do grafo | `pip install networkx` |
| `click` | CLI do Graphify | `pip install click` |
| `jinja2` | Geração do relatório HTML | `pip install jinja2` |

### Verificação rápida (< 5 min)

```bash
cd ~/www/cursos/apps/ana-service
git checkout graphify-baseline-v1
python3 --version                    # esperado: ≥ 3.13
pip install networkx click jinja2
python3 -c "import networkx, click, jinja2; print('OK')"
ls app/services/                     # confirmar estrutura existe
```

## Riscos e Limitações

| # | Risco | Evidência | Mitigação |
|---|---|---|---|
| 1 | **Imports circulares entre services** — Services versionados (`v1`, `v3`) podem importar utils compartilhados que por sua vez importam de volta | `app/services/` tem 167 arquivos em 10 subpastas com provável interdependência | Detectar ciclos com `networkx.find_cycle()` e reportar antes de gerar o grafo completo |
| 2 | **Decorators FastAPI ocultam dependências** — `@router.get()`, `@router.post()` registram rotas dinamicamente; `Depends()` injeta dependências em runtime | `app/routers/` usa decorators em todos os 29 arquivos; `app/dependencies/` tem 5 módulos de injeção | Parsear decorators `@router.*` e chamadas `Depends()` como arestas explícitas no grafo |
| 3 | **Versionamento v1/v3 mascara duplicação** — Código pode ter sido copiado de v1 para v3 com pequenas modificações, gerando duplicação difícil de detectar por comparação textual | Existem `services/v1/` (2 arquivos) e `services/v3/` (26 arquivos) + `skills/v3/` (8 arquivos) | Comparar assinaturas de função + estrutura AST além de texto literal |
| 4 | **Templates Jinja2 são dependências invisíveis** — Services referenciam templates por nome (string), não por import Python | 141 templates em `app/templates/` referenciados via `render()` ou `load()` nos services | Registrar referências string→template como arestas no grafo, mesmo estando fora do escopo de análise Python |

## Ponto de Falha Mais Provável

A detecção de dependências via `Depends()` do FastAPI. Diferente de um import normal (`from app.services.x import Y`), o padrão `Depends(get_current_user)` passa uma *função* como argumento, e a relação só é visível em runtime. Um parser de imports estático não captura isso. Se o Graphify não tratar `Depends()` como uma aresta, o grafo de routers vai parecer isolado dos services — gerando um mapa incompleto que omite exatamente as conexões mais críticas da API.
