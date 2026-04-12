"""
analyze.py — Análise completa do grafo: métricas, comunidades e narrativa arquitetural.

Uso:
    python analyze.py --target-dir ~/www/cursos/apps/ana-service/app
    python analyze.py --target-dir ./app --use-imports   # usa graphify_extract.py (só imports)
"""

import argparse
import json
import logging
import sys
from collections import defaultdict
from pathlib import Path

import community as community_louvain
import igraph as ig
import leidenalg
import networkx as nx

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
log = logging.getLogger("analyze")


# ── Carregar grafo ──────────────────────────────────────────────────────────


def load_graph_from_pipeline(target_dir: Path) -> nx.DiGraph:
    """Constrói grafo via pipeline.py (imports + definições + chamadas)."""
    from pipeline import build_graph
    G, stats = build_graph(target_dir)
    return G


def load_graph_from_extract(target_dir: Path) -> nx.DiGraph:
    """Constrói grafo via graphify_extract.py (só imports)."""
    from graphify_extract import build_graph
    G = build_graph(target_dir)
    return G


# ── Métricas de centralidade ───────────────────────────────────────────────


def compute_metrics(G: nx.DiGraph) -> dict[str, dict]:
    """Calcula in-degree, out-degree, betweenness, PageRank e instabilidade."""
    log.info("Calculando métricas de centralidade...")

    in_deg = dict(G.in_degree())
    out_deg = dict(G.out_degree())

    # Betweenness com amostragem pra grafos grandes
    k = min(200, len(G))
    btw = nx.betweenness_centrality(G, normalized=True, k=k)

    # PageRank
    pr = nx.pagerank(G, alpha=0.85)

    metrics = {}
    for node in G.nodes():
        ca = in_deg[node]
        ce = out_deg[node]
        total = ca + ce
        instability = ce / total if total > 0 else 0

        metrics[node] = {
            "in_degree": ca,
            "out_degree": ce,
            "betweenness": round(btw.get(node, 0), 6),
            "pagerank": round(pr.get(node, 0), 6),
            "instability": round(instability, 4),
        }

    return metrics


# ── Hotspots ────────────────────────────────────────────────────────────────


def find_hotspots(G: nx.DiGraph, metrics: dict, top_n: int = 15) -> list[dict]:
    """Identifica hotspots combinando betweenness, PageRank e in-degree."""
    # Normalização min-max
    all_btw = [m["betweenness"] for m in metrics.values()]
    all_pr = [m["pagerank"] for m in metrics.values()]
    all_in = [m["in_degree"] for m in metrics.values()]

    max_btw = max(all_btw) if all_btw else 1
    max_pr = max(all_pr) if all_pr else 1
    max_in = max(all_in) if all_in else 1

    hotspots = []
    for node, m in metrics.items():
        btw_norm = m["betweenness"] / max_btw if max_btw > 0 else 0
        pr_norm = m["pagerank"] / max_pr if max_pr > 0 else 0
        in_norm = m["in_degree"] / max_in if max_in > 0 else 0

        # Score combinado: peso igual pra cada métrica
        score = (btw_norm + pr_norm + in_norm) / 3

        nd = G.nodes[node]
        hotspots.append({
            "node": node,
            "score": round(score, 6),
            "in_degree": m["in_degree"],
            "out_degree": m["out_degree"],
            "betweenness": m["betweenness"],
            "pagerank": m["pagerank"],
            "instability": m["instability"],
            "kind": nd.get("kind", nd.get("node_type", "")),
            "file": nd.get("file", nd.get("file_path", "")),
        })

    hotspots.sort(key=lambda x: x["score"], reverse=True)
    return hotspots[:top_n]


# ── Componentes fortemente conectados (ciclos) ─────────────────────────────


def find_sccs(G: nx.DiGraph) -> list[set]:
    """Encontra SCCs com mais de 1 nó (indicam ciclos de dependência)."""
    sccs = list(nx.strongly_connected_components(G))
    return [s for s in sccs if len(s) > 1]


# ── Detecção de comunidades ────────────────────────────────────────────────


def detect_communities_leiden(G: nx.DiGraph) -> dict[str, int]:
    """Detecta comunidades com algoritmo Leiden (estado da arte)."""
    log.info("Rodando Leiden para detecção de comunidades...")

    G_und = G.to_undirected()
    nodes = list(G_und.nodes())
    node_idx = {n: i for i, n in enumerate(nodes)}
    edges = [(node_idx[u], node_idx[v]) for u, v in G_und.edges()]

    ig_graph = ig.Graph(n=len(nodes), edges=edges)
    partition = leidenalg.find_partition(ig_graph, leidenalg.ModularityVertexPartition)

    return {
        nodes[i]: part_id
        for part_id, members in enumerate(partition)
        for i in members
    }


def detect_communities_louvain(G: nx.DiGraph) -> dict[str, int]:
    """Fallback: detecta comunidades com Louvain."""
    log.info("Rodando Louvain para detecção de comunidades...")
    G_und = G.to_undirected()
    return community_louvain.best_partition(G_und)


# ── Análise de comunidades ─────────────────────────────────────────────────


def analyze_communities(G: nx.DiGraph, partition: dict[str, int]) -> list[dict]:
    """Analisa cada comunidade: membros, densidade, arestas internas vs externas."""
    clusters = defaultdict(list)
    for node, cid in partition.items():
        clusters[cid].append(node)

    results = []
    for cid, members in sorted(clusters.items(), key=lambda x: len(x[1]), reverse=True):
        member_set = set(members)
        subgraph = G.subgraph(members)

        internal_edges = subgraph.number_of_edges()
        external_edges = sum(
            1 for u, v in G.edges()
            if (u in member_set) != (v in member_set)
            and (u in member_set or v in member_set)
        )

        # Inferir nome semântico baseado nos prefixos mais comuns
        prefixes = defaultdict(int)
        for m in members:
            parts = m.split(".")
            if len(parts) >= 2:
                prefix = ".".join(parts[:2])
                prefixes[prefix] += 1

        top_prefix = max(prefixes, key=prefixes.get) if prefixes else "unknown"

        # Densidade do subgrafo
        n = len(members)
        max_edges = n * (n - 1)  # grafo dirigido
        density = internal_edges / max_edges if max_edges > 0 else 0

        results.append({
            "cluster_id": cid,
            "size": len(members),
            "inferred_name": top_prefix,
            "top_members": sorted(members)[:10],
            "internal_edges": internal_edges,
            "external_edges": external_edges,
            "density": round(density, 4),
            "cohesion_ratio": round(
                internal_edges / (internal_edges + external_edges)
                if (internal_edges + external_edges) > 0 else 0, 4
            ),
        })

    return results


# ── Narrativa arquitetural ─────────────────────────────────────────────────


def generate_narrative(
    G: nx.DiGraph,
    hotspots: list[dict],
    communities: list[dict],
    sccs: list[set],
    metrics: dict,
) -> str:
    """Gera narrativa arquitetural baseada em evidências do grafo."""
    lines = []
    lines.append("# Narrativa Arquitetural — ana-service")
    lines.append("")
    lines.append(f"**Grafo:** {G.number_of_nodes()} nós, {G.number_of_edges()} arestas")
    lines.append(f"**Comunidades detectadas (Leiden):** {len(communities)}")
    lines.append(f"**Ciclos de dependência (SCCs > 1):** {len(sccs)}")
    lines.append("")

    # Camadas identificadas
    lines.append("## Camadas Identificadas")
    lines.append("")
    for c in communities[:8]:
        ratio_pct = round(c["cohesion_ratio"] * 100)
        lines.append(
            f"- **{c['inferred_name']}** — {c['size']} nós, "
            f"coesão {ratio_pct}% ({c['internal_edges']} arestas internas, "
            f"{c['external_edges']} externas)"
        )
        if c["top_members"]:
            for m in c["top_members"][:5]:
                lines.append(f"  - `{m}`")
    lines.append("")

    # Hotspots
    lines.append("## Hotspots Críticos")
    lines.append("")
    for i, h in enumerate(hotspots[:5], 1):
        risk_label = "PONTE" if h["betweenness"] > 0.05 else "HUB" if h["in_degree"] > 10 else "MODERADO"
        lines.append(
            f"{i}. **`{h['node']}`** [{risk_label}]  "
        )
        lines.append(
            f"   in_degree={h['in_degree']}, betweenness={h['betweenness']}, "
            f"pagerank={h['pagerank']}, I={h['instability']}"
        )
        if h["file"]:
            lines.append(f"   Arquivo: `{h['file']}`")
        lines.append("")

    # Ciclos
    if sccs:
        lines.append("## Ciclos de Dependência")
        lines.append("")
        for i, scc in enumerate(sccs[:5], 1):
            lines.append(f"{i}. Ciclo com {len(scc)} nós: {', '.join(list(scc)[:5])}")
        lines.append("")

    # Fluxo principal
    lines.append("## Fluxo Principal Inferido")
    lines.append("")

    # Encontrar o nó com maior out-degree (provavelmente entrypoint)
    entry = max(metrics.items(), key=lambda x: x[1]["out_degree"])
    hub = max(metrics.items(), key=lambda x: x[1]["in_degree"])

    lines.append(
        f"Entrypoint provável: **`{entry[0]}`** (out_degree={entry[1]['out_degree']}, I={entry[1]['instability']})"
    )
    lines.append(
        f"Hub central: **`{hub[0]}`** (in_degree={hub[1]['in_degree']}, I={hub[1]['instability']})"
    )
    lines.append("")
    lines.append(
        f"Fluxo: Request → `{entry[0]}` → routers → services → `{hub[0]}` → providers"
    )

    return "\n".join(lines)


# ── Export ──────────────────────────────────────────────────────────────────


def export_report(
    hotspots: list[dict],
    communities: list[dict],
    sccs: list[set],
    narrative: str,
    output_path: Path,
) -> None:
    """Exporta relatório completo em JSON."""
    report = {
        "hotspots": hotspots,
        "communities": [
            {k: v for k, v in c.items() if k != "top_members"}
            | {"sample_members": c["top_members"][:5]}
            for c in communities
        ],
        "dependency_cycles": [
            list(scc)[:10] for scc in sccs[:10]
        ],
        "narrative": narrative,
    }

    with open(output_path, "w") as f:
        json.dump(report, f, indent=2, ensure_ascii=False)
    log.info(f"Relatório exportado: {output_path}")


# ── CLI ─────────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Analyze — Métricas, comunidades e narrativa arquitetural"
    )
    parser.add_argument(
        "--target-dir", type=Path, required=True, help="Diretório para analisar"
    )
    parser.add_argument(
        "--use-imports", action="store_true",
        help="Usar graphify_extract.py (só imports) em vez do pipeline completo"
    )
    parser.add_argument(
        "--output", type=Path, default=Path("analysis_report.json"),
        help="Arquivo de saída (default: analysis_report.json)"
    )
    args = parser.parse_args()

    if not args.target_dir.is_dir():
        log.error(f"Diretório não encontrado: {args.target_dir}")
        sys.exit(1)

    # Carregar grafo
    if args.use_imports:
        log.info("Usando graphify_extract.py (só imports)...")
        G = load_graph_from_extract(args.target_dir)
    else:
        log.info("Usando pipeline completo (imports + definições + chamadas)...")
        G = load_graph_from_pipeline(args.target_dir)

    # Métricas
    metrics = compute_metrics(G)

    # Hotspots
    hotspots = find_hotspots(G, metrics, top_n=15)

    # SCCs (ciclos)
    sccs = find_sccs(G)
    log.info(f"SCCs com ciclos: {len(sccs)}")

    # Comunidades (Leiden)
    try:
        partition = detect_communities_leiden(G)
    except Exception as e:
        log.warning(f"Leiden falhou ({e}), usando Louvain como fallback...")
        partition = detect_communities_louvain(G)

    communities = analyze_communities(G, partition)
    log.info(f"Comunidades detectadas: {len(communities)}")

    # Narrativa
    narrative = generate_narrative(G, hotspots, communities, sccs, metrics)

    # Exportar
    export_report(hotspots, communities, sccs, narrative, args.output)

    # Sumário no terminal
    print(f"\n{'='*60}")
    print(f"Análise Arquitetural — Sumário")
    print(f"{'='*60}")
    print(f"Nós:          {G.number_of_nodes()}")
    print(f"Arestas:      {G.number_of_edges()}")
    print(f"Comunidades:  {len(communities)}")
    print(f"Ciclos (SCC): {len(sccs)}")

    print(f"\nTop 5 Hotspots:")
    for i, h in enumerate(hotspots[:5], 1):
        print(f"  {i}. {h['node']}")
        print(f"     score={h['score']}  in={h['in_degree']}  btw={h['betweenness']}  pr={h['pagerank']}  I={h['instability']}")

    print(f"\nComunidades (top 5):")
    for c in communities[:5]:
        print(f"  [{c['cluster_id']}] {c['inferred_name']} — {c['size']} nós, coesão={c['cohesion_ratio']}")

    # Salvar narrativa como markdown também
    narrative_path = args.output.with_suffix(".md")
    with open(narrative_path, "w") as f:
        f.write(narrative)
    print(f"\nNarrativa: {narrative_path}")
    print(f"Relatório: {args.output}")


if __name__ == "__main__":
    main()
