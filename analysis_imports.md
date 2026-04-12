# Narrativa Arquitetural — ana-service

**Grafo:** 375 nós, 1897 arestas
**Comunidades detectadas (Leiden):** 25
**Ciclos de dependência (SCCs > 1):** 3

## Camadas Identificadas

- **app.services** — 66 nós, coesão 39% (151 arestas internas, 234 externas)
  - `PIL`
  - `PyPDF2`
  - `app.constants.llm_operations`
  - `app.dependencies.context7`
  - `app.errors.llm`
- **app.skills** — 54 nós, coesão 36% (163 arestas internas, 295 externas)
  - `abc`
  - `app.routers.skills`
  - `app.services.llm.factory`
  - `app.services.llm.mcp_service`
  - `app.services.study.constants`
- **app.services** — 53 nós, coesão 31% (172 arestas internas, 379 externas)
  - `anthropic`
  - `anthropic.types`
  - `app.config`
  - `app.errors`
  - `app.jobs.context_summarizer`
- **app.services** — 49 nós, coesão 41% (113 arestas internas, 160 externas)
  - `app.context.context_loader`
  - `app.jobs.generate_course_job`
  - `app.logging_http`
  - `app.main`
  - `app.models.bible`
- **app.routers** — 48 nós, coesão 35% (139 arestas internas, 257 externas)
  - `app.dependencies`
  - `app.dependencies.auth`
  - `app.dependencies.llm`
  - `app.dependencies.llm_context`
  - `app.middleware.tracing`
- **app.services** — 36 nós, coesão 39% (102 arestas internas, 162 externas)
  - `app.models.pipeline_v3`
  - `app.routers.generation_v3`
  - `app.services.context_prioritizer`
  - `app.services.npm_registry_client`
  - `app.services.prompt_compressor`
- **app.services** — 34 nós, coesão 28% (110 arestas internas, 279 externas)
  - `app.constants`
  - `app.prompts.loader`
  - `app.prompts.tutor_system_prompt`
  - `app.routers.llm`
  - `app.services.course_metadata_extractor`
- **app.agents** — 18 nós, coesão 52% (44 arestas internas, 40 externas)
  - `app.agents`
  - `app.agents.base`
  - `app.agents.factory`
  - `app.agents.pipeline`
  - `app.agents.schemas`

## Hotspots Críticos

1. **`typing`** [HUB]  
   in_degree=200, betweenness=0.0, pagerank=0.073439, I=0.0

2. **`logging`** [HUB]  
   in_degree=182, betweenness=0.0, pagerank=0.051009, I=0.0

3. **`app.services.llm.factory`** [MODERADO]  
   in_degree=7, betweenness=0.015895, pagerank=0.0033, I=0.5
   Arquivo: `app/services/llm/factory.py`

4. **`app.skills.runtime`** [HUB]  
   in_degree=14, betweenness=0.012287, pagerank=0.005487, I=0.3
   Arquivo: `app/skills/runtime.py`

5. **`app.middleware.tracing`** [HUB]  
   in_degree=53, betweenness=0.006193, pagerank=0.009169, I=0.1587
   Arquivo: `app/middleware/tracing.py`

## Ciclos de Dependência

1. Ciclo com 3 nós: app.monitoring, app.services.redis_client, app.services.context7_client
2. Ciclo com 2 nós: app.services.claude_client, app.services.streaming
3. Ciclo com 8 nós: app.services.validators.froebel, app.services.validators.herbart, app.services.validators, app.services.validators.montessori, app.services.validators.language

## Fluxo Principal Inferido

Entrypoint provável: **`app.routers.generation_v3`** (out_degree=44, I=1.0)
Hub central: **`typing`** (in_degree=200, I=0.0)

Fluxo: Request → `app.routers.generation_v3` → routers → services → `typing` → providers