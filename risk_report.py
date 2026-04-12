"""
risk_report.py — Relatório de risco arquitetural baseado no grafo de símbolos.

Análises:
  1. Detecção de ciclos no subgrafo de CALLS
  2. Betweenness centrality + in-degree centrality
  3. Top 5 símbolos de maior risco (combinando métricas)

Uso:
    python risk_report.py --target-dir ~/www/cursos/apps/ana-service/app
"""

import argparse
import json
import logging
import sys
from pathlib import Path

import networkx as nx

from pipeline import build_graph

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
log = logging.getLogger("risk_report")


def detect_cycles(G: nx.DiGraph) -> list[list[str]]:
    """Detecta ciclos no subgrafo de arestas CALLS."""
    call_edges = [(u, v) for u, v, d in G.edges(data=True) if d.get("edge_type") == "calls"]

    if not call_edges:
        return []

    call_subgraph = G.edge_subgraph(call_edges).copy()

    # simple_cycles pode ser custoso em grafos grandes — limita a 500
    cycles = []
    try:
        for i, cycle in enumerate(nx.simple_cycles(call_subgraph)):
            cycles.append(cycle)
            if i >= 499:
                log.warning("Limite de 500 ciclos atingido, interrompendo busca")
                break
    except nx.NetworkXError as e:
        log.warning(f"Erro na detecção de ciclos: {e}")

    return cycles


def compute_centrality(G: nx.DiGraph) -> dict[str, dict]:
    """Calcula betweenness e in-degree centrality para funções e classes."""
    # Filtra só nós de função/classe
    symbol_nodes = [n for n, d in G.nodes(data=True) if d.get("kind") in ("function", "class")]

    if not symbol_nodes:
        return {}

    subgraph = G.subgraph(symbol_nodes)

    betweenness = nx.betweenness_centrality(subgraph)
    in_degree_cent = nx.in_degree_centrality(subgraph)

    metrics = {}
    for node in symbol_nodes:
        metrics[node] = {
            "betweenness": round(betweenness.get(node, 0), 6),
            "in_degree": G.in_degree(node),
            "in_degree_centrality": round(in_degree_cent.get(node, 0), 6),
        }

    return metrics


def rank_risk(G: nx.DiGraph, metrics: dict, cycles: list[list[str]]) -> list[dict]:
    """Combina métricas para ranquear os 5 símbolos de maior risco.

    Critério de combinação (risk_score):
        risk_score = (betweenness_norm * 0.4) + (in_degree_norm * 0.4) + (in_cycle * 0.2)

    Justificativa:
    - betweenness alta = nó ponte entre muitas dependências → mudança propaga amplamente
    - in_degree alto = muitos módulos dependem desse símbolo → impacto amplo se mudar
    - participação em ciclo = dependência circular → risco de quebra em cascata
    - Pesos iguais pra betweenness e in_degree (0.4 cada) porque ambas medem impacto
    - Ciclo tem peso menor (0.2) porque é binário (sim/não), não gradual
    """
    # Nós que participam de ciclos
    cycle_nodes = set()
    for cycle in cycles:
        cycle_nodes.update(cycle)

    # Normalizar métricas (min-max scaling)
    all_betweenness = [m["betweenness"] for m in metrics.values()]
    all_in_degree = [m["in_degree"] for m in metrics.values()]

    max_bet = max(all_betweenness) if all_betweenness else 1
    max_in = max(all_in_degree) if all_in_degree else 1

    ranked = []
    for node, m in metrics.items():
        node_data = G.nodes[node]

        # Exclui nós sem rastreabilidade
        if "file" not in node_data or "line" not in node_data:
            continue

        bet_norm = m["betweenness"] / max_bet if max_bet > 0 else 0
        in_norm = m["in_degree"] / max_in if max_in > 0 else 0
        in_cycle = 1.0 if node in cycle_nodes else 0.0

        risk_score = (bet_norm * 0.4) + (in_norm * 0.4) + (in_cycle * 0.2)

        ranked.append({
            "name": node,
            "file": node_data.get("file", ""),
            "line": node_data.get("line", 0),
            "kind": node_data.get("kind", ""),
            "betweenness": m["betweenness"],
            "in_degree": m["in_degree"],
            "in_cycles": node in cycle_nodes,
            "risk_score": round(risk_score, 6),
        })

    ranked.sort(key=lambda x: x["risk_score"], reverse=True)
    return ranked[:5]


def generate_report(target_dir: Path) -> dict:
    """Gera relatório completo de risco."""
    G, stats = build_graph(target_dir)

    log.info("Detectando ciclos...")
    cycles = detect_cycles(G)
    log.info(f"Ciclos encontrados: {len(cycles)}")

    log.info("Calculando centralidade...")
    metrics = compute_centrality(G)

    log.info("Ranqueando riscos...")
    top5 = rank_risk(G, metrics, cycles)

    # Amostra de ciclos (máx 10, com metadados)
    cycles_sample = []
    for cycle in cycles[:10]:
        cycle_with_meta = []
        for node in cycle:
            nd = G.nodes.get(node, {})
            cycle_with_meta.append({
                "name": node,
                "file": nd.get("file", ""),
                "line": nd.get("line", 0),
            })
        cycles_sample.append(cycle_with_meta)

    report = {
        "target_dir": str(target_dir),
        "total_nodes": G.number_of_nodes(),
        "total_edges": G.number_of_edges(),
        "total_cycles": len(cycles),
        "cycles_sample": cycles_sample,
        "top5_risk_symbols": top5,
        "methodology": {
            "description": "risk_score = (betweenness_norm * 0.4) + (in_degree_norm * 0.4) + (in_cycle * 0.2)",
            "betweenness_weight": 0.4,
            "in_degree_weight": 0.4,
            "cycle_weight": 0.2,
            "normalization": "min-max scaling (0-1) para betweenness e in_degree",
            "rationale": (
                "Betweenness e in_degree recebem peso igual (0.4 cada) porque ambas medem "
                "impacto de mudança: betweenness mede propagação (nó ponte), in_degree mede "
                "dependência direta (quantos módulos usam). Participação em ciclo recebe "
                "peso menor (0.2) porque é binário, mas indica risco de quebra em cascata."
            ),
        },
    }

    return report


# ── CLI ─────────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Risk Report — Análise de risco arquitetural via grafo de símbolos"
    )
    parser.add_argument(
        "--target-dir", type=Path, required=True, help="Diretório para analisar"
    )
    parser.add_argument(
        "--output", type=Path, default=Path("risk_report.json"), help="Arquivo de saída (default: risk_report.json)"
    )
    args = parser.parse_args()

    if not args.target_dir.is_dir():
        log.error(f"Diretório não encontrado: {args.target_dir}")
        sys.exit(1)

    report = generate_report(args.target_dir)

    with open(args.output, "w") as f:
        json.dump(report, f, indent=2)

    # Sumário
    print(f"\n{'='*60}")
    print(f"Risk Report — Sumário")
    print(f"{'='*60}")
    print(f"Nós:    {report['total_nodes']}")
    print(f"Arestas: {report['total_edges']}")
    print(f"Ciclos:  {report['total_cycles']}")

    print(f"\nTop 5 Símbolos de Risco:")
    for i, s in enumerate(report["top5_risk_symbols"], 1):
        cycle_marker = " [CICLO]" if s["in_cycles"] else ""
        print(f"  {i}. {s['name']}")
        print(f"     {s['file']}:{s['line']}  |  betweenness={s['betweenness']}  in_degree={s['in_degree']}{cycle_marker}")
        print(f"     risk_score={s['risk_score']}")

    print(f"\nRelatório salvo em: {args.output}")


if __name__ == "__main__":
    main()
