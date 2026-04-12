"""
Graphify — Grafo enriquecido com propriedades (Módulo 3, Tier Semi-guiado)
Dados reais do ana-service com metadados por nó.
"""

import json
import csv
import networkx as nx
from networkx.readwrite import json_graph

# ── Imports reais do ana-service ──────────────────────────────────────────────

imports = [
    ("app.main", "app.config"),
    ("app.main", "app.version"),
    ("app.main", "app.routers.health"),
    ("app.main", "app.routers.courses"),
    ("app.main", "app.routers.spark_chat"),
    ("app.main", "app.routers.admin"),
    ("app.main", "app.routers.skills"),
    ("app.main", "app.routers.generation_v3"),
    ("app.main", "app.routers.study_course"),
    ("app.main", "app.services.claude_client"),
    ("app.main", "app.services.rate_limiter"),
    ("app.main", "app.services.database"),
    ("app.main", "app.services.project_cost_aggregator"),
    ("app.main", "app.middleware.tracing"),
    ("app.main", "app.routers.llm"),
    ("app.main", "app.routers.mcp"),
    ("app.services.v3.pipeline_orchestrator", "app.models.pipeline_v3"),
    ("app.services.v3.pipeline_orchestrator", "app.skills"),
    ("app.services.v3.pipeline_orchestrator", "app.services.llm"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.bible_generator"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.domain_classifier"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.graph_filter"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.idea_spec_builder"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.knowledge_graph_extractor"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.lesson_generation_runner"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.plan_reviewer"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.variant_plan_builder"),
    ("app.services.v3.pipeline_orchestrator", "app.services.v3.wave_orchestrator"),
    ("app.services.study.study_course_service", "app.services.llm"),
    ("app.services.study.study_course_service", "app.services.study.course_helpers"),
    ("app.services.study.study_course_service", "app.services.study.course_repository"),
    ("app.services.study.study_course_service", "app.services.study.message_processor"),
    ("app.services.study.study_course_service", "app.services.study.specialist_router"),
    ("app.services.study.study_course_service", "app.services.llm_tracker"),
    ("app.services.study.study_course_service", "app.services.redis_client"),
    ("app.services.study.study_course_service", "app.middleware.tracing"),
    ("app.services.study.study_course_service", "app.utils"),
]

# ── Metadados dos nós ────────────────────────────────────────────────────────

node_metadata = {
    "app.main":                                    {"node_type": "entrypoint", "layer": "app"},
    "app.config":                                  {"node_type": "config",     "layer": "config"},
    "app.version":                                 {"node_type": "config",     "layer": "config"},
    "app.middleware.tracing":                       {"node_type": "middleware", "layer": "middleware"},
    "app.utils":                                   {"node_type": "utility",    "layer": "utility"},
    "app.models.pipeline_v3":                      {"node_type": "model",      "layer": "models"},
    "app.skills":                                  {"node_type": "module",     "layer": "skills"},
    "app.services.llm":                            {"node_type": "module",     "layer": "services"},
    "app.services.llm_tracker":                    {"node_type": "module",     "layer": "services"},
    "app.services.redis_client":                   {"node_type": "module",     "layer": "services"},
    "app.services.claude_client":                  {"node_type": "module",     "layer": "services"},
    "app.services.rate_limiter":                   {"node_type": "module",     "layer": "services"},
    "app.services.database":                       {"node_type": "module",     "layer": "services"},
    "app.services.project_cost_aggregator":        {"node_type": "module",     "layer": "services"},
    "app.services.v3.pipeline_orchestrator":       {"node_type": "orchestrator", "layer": "services.v3"},
    "app.services.v3.bible_generator":             {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.domain_classifier":           {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.graph_filter":                {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.idea_spec_builder":           {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.knowledge_graph_extractor":   {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.lesson_generation_runner":    {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.plan_reviewer":               {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.variant_plan_builder":        {"node_type": "module",     "layer": "services.v3"},
    "app.services.v3.wave_orchestrator":           {"node_type": "module",     "layer": "services.v3"},
    "app.services.study.study_course_service":     {"node_type": "orchestrator", "layer": "services.study"},
    "app.services.study.course_helpers":            {"node_type": "module",     "layer": "services.study"},
    "app.services.study.course_repository":         {"node_type": "module",     "layer": "services.study"},
    "app.services.study.message_processor":         {"node_type": "module",     "layer": "services.study"},
    "app.services.study.specialist_router":         {"node_type": "module",     "layer": "services.study"},
}

# Routers — gerar automaticamente
for source, target in imports:
    if target.startswith("app.routers."):
        node_metadata.setdefault(target, {"node_type": "router", "layer": "routers"})

# ── Construir grafo ──────────────────────────────────────────────────────────

G = nx.DiGraph()
G.add_edges_from(imports)

# Adicionar propriedades nos nós
for node in G.nodes():
    meta = node_metadata.get(node, {"node_type": "unknown", "layer": "unknown"})
    G.nodes[node]["node_type"] = meta["node_type"]
    G.nodes[node]["layer"] = meta["layer"]
    G.nodes[node]["in_degree"] = G.in_degree(node)
    G.nodes[node]["out_degree"] = G.out_degree(node)

# ── Métricas ─────────────────────────────────────────────────────────────────

print(f"Nós: {G.number_of_nodes()}")
print(f"Arestas: {G.number_of_edges()}")

print(f"\n{'Nó':<55} {'Tipo':<15} {'Camada':<18} {'In':>3} {'Out':>3}")
print("─" * 98)

rows = sorted(G.nodes(data=True), key=lambda x: x[1].get("in_degree", 0) + x[1].get("out_degree", 0), reverse=True)
for node, data in rows:
    print(f"{node:<55} {data['node_type']:<15} {data['layer']:<18} {data['in_degree']:>3} {data['out_degree']:>3}")

# ── Exportar JSON enriquecido ────────────────────────────────────────────────

data = json_graph.node_link_data(G)
with open("graph.json", "w") as f:
    json.dump(data, f, indent=2)
print("\ngraph.json gerado (enriquecido com propriedades).")

# ── Exportar CSV ─────────────────────────────────────────────────────────────

with open("edges.csv", "w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow(["source", "target"])
    for s, t in G.edges():
        writer.writerow([s, t])
print("edges.csv gerado.")
