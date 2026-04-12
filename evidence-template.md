# Evidence Template — ana-service Exploration

## Ponto de Entrada

- **Path:** `app/main.py`
- **Framework:** FastAPI
- **Função de criação:** `create_app()` (linha 399)
- **Lifespan:** `lifespan()` (linha 334) — inicializa Redis, rate limiter, config subscriber, ProjectCostAggregator

## Módulos Identificados

| Módulo | Path | Arquivos | Responsabilidade |
|---|---|---|---|
| routers | `app/routers/` | 29 | Endpoints da API — ponto de entrada de cada feature |
| services/v3 | `app/services/v3/` | 26 | Pipeline de geração de cursos v3 (Idea → Extract → Variant → Generate) |
| services/study | `app/services/study/` | 20 | Sessões de estudo com tutor IA — orquestração, progresso, streaming |
| services/llm | `app/services/llm/` | 22 | Gateway LLM, providers (Claude, OpenAI, Gemini), fallback, roteamento |
| skills | `app/skills/` | 30 | Orquestrações de alto nível (study, v3, video) |
| agents | `app/agents/` | 10 | Personas, stages, schemas de agentes |
| models | `app/models/` | 9 | Schemas Pydantic — contratos de dados |
| middleware | `app/middleware/` | 3 | Tracing (correlation ID) e tracking de LLM |
| config | `app/config.py` | 1 | Settings centralizadas (Pydantic Settings) |

## Dependências Externas Principais

| Pacote | Propósito | Referência |
|---|---|---|
| fastapi | Framework web | `app/main.py:25` |
| anthropic | Claude API client | `requirements.txt` |
| openai | OpenAI API client | `requirements.txt` |
| google-genai | Gemini API client | `requirements.txt` |
| sentry_sdk | Monitoramento de erros | `app/main.py:19` |
| redis | Cache, rate limiting, pub/sub | `app/main.py:337` |
| sqlalchemy + asyncpg | Banco de dados | `requirements.txt` |
| networkx | (nenhum uso atual — será usado pelo Graphify) | — |

## Dependências Internas (Referências Cruzadas)

### main.py → (fan-out: 26 módulos)
- `app.config.get_settings` → `app/main.py:29`
- `app.version.get_app_version` → `app/main.py:30`
- `app.routers.*` (22 routers) → `app/main.py:130`
- `app.services.claude_client` → `app/main.py:139`
- `app.services.rate_limiter` → `app/main.py:144`
- `app.middleware.tracing` → `app/main.py:138`

### pipeline_orchestrator.py → (fan-out: 13 módulos)
- `app.models.pipeline_v3` (11 modelos) → `app/services/v3/pipeline_orchestrator.py:18`
- `app.skills` → `app/services/v3/pipeline_orchestrator.py:32`
- `app.services.llm.base.LLMClient` → `app/services/v3/pipeline_orchestrator.py:34`
- `app.services.v3.*` (10 módulos v3 internos) → `app/services/v3/pipeline_orchestrator.py:35-45`

### study_course_service.py → (fan-out: 10 módulos)
- `app.services.llm` (6 tipos) → `app/services/study/study_course_service.py:22`
- `app.services.study.*` (4 submódulos) → `app/services/study/study_course_service.py:30-47`
- `app.services.llm_tracker` → `app/services/study/study_course_service.py:48`
- `app.services.redis_client` → `app/services/study/study_course_service.py:49`
- `app.middleware.tracing` → `app/services/study/study_course_service.py:50`
- `app.utils` → `app/services/study/study_course_service.py:51`

## Hipótese de Fluxo Principal

O ana-service opera em dois fluxos distintos:
1. **Geração (v3):** Request HTTP → router → pipeline_orchestrator → (idea_spec → knowledge_graph → variant_plan → lesson_generation) → LLM providers → response
2. **Estudo (study):** Request HTTP → router → study_course_service → (specialist_router → tutor_service → progress_service) → LLM streaming → SSE response

Ambos convergem em `app.services.llm` como camada de abstração LLM, mas usam interfaces diferentes (batch vs streaming).

### services/llm/base.py → Camada de abstração LLM
- Define `Message`, `CacheConfig`, `LLMClient` (ABC) → `app/services/llm/base.py:1-40`
- Interface unificada para Anthropic, OpenAI, Gemini
- Consumido por: `services/v3` (via LLMClient), `services/study` (via Message/TextEvent)
- **Nó central do grafo** — todos os fluxos convergem aqui

### skills/__init__.py → Framework de skills executáveis
- Exporta: `SkillDefinition`, `SkillExecutor`, `SkillRegistry`, `SkillSelector` → `app/skills/__init__.py:10-30`
- Usa imports lazy pra evitar ciclos (`from app.skills.base import ...`) → `app/skills/__init__.py:6`
- Consumido por: `services/v3/pipeline_orchestrator` (via SkillExecutor)

### agents/__init__.py → Pipeline multi-agente
- Pipeline sequencial: Planner → Generator → Reviewer → Refiner → `app/agents/__init__.py:4`
- Usa imports lazy via factory functions (linhas 14-23) pra evitar ciclos
- Feature flag: `AGENT_PIPELINE_ENABLED` (default: false) → `app/agents/__init__.py:7`
- Depende de: `app.agents.base`, `app.agents.pipeline`, `app.agents.factory`

## Módulo Hub — app.services.llm

`app.services.llm` aparece como dependência em pelo menos 3 módulos distintos:
- `services/v3/pipeline_orchestrator.py:34` → usa `LLMClient`
- `services/study/study_course_service.py:22` → usa `Message`, `TextEvent`, `ToolUseEvent`
- `skills/` → provavelmente usa via SkillExecutor

Isso configura um **hub de alto fan-in**: muitos módulos dependem dele. Se a interface de `LLMClient` mudar, o impacto se propaga pra geração, estudo e skills. No grafo, esse nó vai ter a maior centralidade de intermediação (betweenness centrality).

## Sinais de Investigação para o Graphify

| Sinal | Evidência | Prioridade |
|---|---|---|
| `_acquire_with_retry` pode estar duplicada | `app/services/study/study_course_service.py:57` — implementação local de retry | Alta |
| `v3` e `study` usam `app.services.llm` por interfaces diferentes | v3: `LLMClient` (base), study: `Message/TextEvent/ToolUseEvent` (streaming) | Média |
| `constants.py` existe em `v3/` e `study/` | Possível duplicação de constantes compartilháveis | Média |
| `app.middleware.tracing` é importado em múltiplos níveis | main.py + study_course_service + possivelmente outros | Baixa (pode ser intencional) |
| Imports lazy em `skills/` e `agents/` pra evitar ciclos | `app/skills/__init__.py:6`, `app/agents/__init__.py:14-23` — factory functions com import interno | Alta — indica risco de dependência circular |
| `app.services.llm` como hub de fan-in alto | Consumido por v3, study, skills — 3+ módulos distintos | Alta — mudança na interface propaga amplamente |

## Ferramenta Escolhida para Exploração

**Claude Code** — justificativa:
- Contexto de 1M tokens permite carregar múltiplos arquivos e cruzar dependências
- Acesso direto ao filesystem para navegar o repositório inteiro
- Respostas com evidência (path + linha) verificáveis
- Ideal pra exploração ampla multi-módulo, que é exatamente o que o Graphify exige
