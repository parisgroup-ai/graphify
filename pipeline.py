"""
pipeline.py — Pipeline de extração de símbolos para diretório inteiro.

Percorre todos os .py de um diretório, extrai definições + chamadas,
constrói grafo normalizado com NetworkX e exporta CSV + relatório de qualidade.

Uso:
    python pipeline.py --target-dir ~/www/cursos/apps/ana-service/app
    python pipeline.py --target-dir ./app --output-dir ./output
"""

import argparse
import csv
import json
import logging
import sys
from pathlib import Path

import networkx as nx

from extractor import create_parser, extract_calls, extract_symbols

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
log = logging.getLogger("pipeline")

# ── Diretórios excluídos ────────────────────────────────────────────────────

EXCLUDED_DIRS = {"__pycache__", "node_modules", ".git", "dist", ".next", "tests", "__tests__"}


# ── File discovery ──────────────────────────────────────────────────────────


def find_python_files(target_dir: Path) -> list[Path]:
    """Encontra todos os .py excluindo diretórios irrelevantes."""
    result = []
    for p in sorted(target_dir.rglob("*.py")):
        if not set(p.parts) & EXCLUDED_DIRS:
            result.append(p)
    return result


# ── Normalização ────────────────────────────────────────────────────────────


def filepath_to_module(filepath: Path, target_dir: Path) -> str:
    """Converte path em módulo canônico: app/services/llm.py → app.services.llm"""
    relative = filepath.relative_to(target_dir.parent)
    parts = list(relative.with_suffix("").parts)
    if parts and parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts)


def canonical_name(module: str, symbol_name: str) -> str:
    """Gera ID canônico: app.services.llm.LLMClient"""
    return f"{module}.{symbol_name}"


# ── Construção do grafo ────────────────────────────────────────────────────


def build_graph(target_dir: Path) -> tuple[nx.DiGraph, dict]:
    """Extrai símbolos e chamadas de todos os .py e constrói o grafo.

    Retorna (grafo, stats) onde stats tem contadores de deduplicação.
    """
    parser = create_parser()
    files = find_python_files(target_dir)
    log.info(f"Encontrados {len(files)} arquivos Python em {target_dir}")

    G = nx.DiGraph()

    # Índice de símbolos: nome_simples → [canonical_ids]
    # Usado pra resolver chamadas → definições
    def_index: dict[str, list[str]] = {}

    all_symbols = []
    all_calls = []

    # ── Fase 1: extrair tudo ────────────────────────────────────────────

    for f in files:
        try:
            source = f.read_bytes()
        except (OSError, IOError) as e:
            log.warning(f"Não consegui ler {f}: {e}")
            continue

        try:
            tree = parser.parse(source)
        except Exception as e:
            log.warning(f"Erro de parse em {f}: {e}")
            continue

        module = filepath_to_module(f, target_dir)
        rel_path = str(f.relative_to(target_dir.parent))

        symbols = extract_symbols(source, rel_path, tree)
        calls = extract_calls(source, rel_path, tree)

        # Adiciona o módulo ao contexto de cada símbolo
        for s in symbols:
            s["module"] = module
            s["canonical"] = canonical_name(module, s["name"])
            all_symbols.append(s)

            # Registra no índice de definições
            def_index.setdefault(s["name"], []).append(s["canonical"])

        for c in calls:
            c["module"] = module
            all_calls.append(c)

    log.info(f"Extraídos {len(all_symbols)} definições e {len(all_calls)} chamadas")

    # ── Fase 2: construir nós ───────────────────────────────────────────

    # Nós de arquivo (módulo)
    for f in files:
        module = filepath_to_module(f, target_dir)
        rel_path = str(f.relative_to(target_dir.parent))
        G.add_node(module, kind="module", file=rel_path, line=0)

    # Nós de símbolo (função/classe)
    for s in all_symbols:
        G.add_node(s["canonical"], kind=s["kind"], file=s["file"], line=s["line"])

    # ── Fase 3: construir arestas com deduplicação ──────────────────────

    seen_edges: set[tuple[str, str, str]] = set()
    duplicates_removed = 0

    # Arestas DEFINES: módulo → símbolo
    for s in all_symbols:
        edge_key = (s["module"], s["canonical"], "defines")
        if edge_key not in seen_edges:
            seen_edges.add(edge_key)
            G.add_edge(s["module"], s["canonical"], edge_type="defines", file=s["file"], line=s["line"])
        else:
            duplicates_removed += 1

    # Arestas CALLS: módulo_que_chama → símbolo_chamado
    for c in all_calls:
        callee_name = c["callee_name"]

        # Resolve: o nome chamado bate com algum símbolo conhecido?
        targets = def_index.get(callee_name, [])

        if not targets:
            continue  # chamada a símbolo externo (stdlib, lib) — ignora

        for target in targets:
            edge_key = (c["module"], target, "calls")
            if edge_key not in seen_edges:
                seen_edges.add(edge_key)
                if G.has_edge(c["module"], target):
                    G[c["module"]][target]["weight"] = G[c["module"]][target].get("weight", 1) + 1
                else:
                    G.add_edge(c["module"], target, edge_type="calls", weight=1, file=c["file"], line=c["line"])
            else:
                duplicates_removed += 1

    log.info(f"Grafo: {G.number_of_nodes()} nós, {G.number_of_edges()} arestas ({duplicates_removed} duplicatas removidas)")

    stats = {
        "total_files": len(files),
        "total_symbols": len(all_symbols),
        "total_calls": len(all_calls),
        "duplicates_removed": duplicates_removed,
    }

    return G, stats


# ── Relatório de qualidade ──────────────────────────────────────────────────


def quality_report(G: nx.DiGraph, stats: dict) -> dict:
    """Gera relatório de qualidade do grafo."""
    # Nós sem rastreabilidade (sem atributo line)
    nodes_without_line = [n for n, d in G.nodes(data=True) if "line" not in d]

    # Símbolos duplicados: mesmo nome simples em arquivos diferentes
    name_to_files: dict[str, list[str]] = {}
    for n, d in G.nodes(data=True):
        if d.get("kind") in ("function", "class"):
            simple_name = n.rsplit(".", 1)[-1]
            name_to_files.setdefault(simple_name, []).append(n)

    duplicated_names = {name: ids for name, ids in name_to_files.items() if len(ids) > 1}

    return {
        "total_nodes": G.number_of_nodes(),
        "total_edges": G.number_of_edges(),
        "nodes_without_line": nodes_without_line,
        "duplicated_symbol_names": {
            name: ids for name, ids in sorted(duplicated_names.items(), key=lambda x: len(x[1]), reverse=True)[:20]
        },
        "duplicated_symbol_count": len(duplicated_names),
        "duplicates_removed": stats["duplicates_removed"],
        "total_files": stats["total_files"],
        "total_symbols_extracted": stats["total_symbols"],
        "total_calls_extracted": stats["total_calls"],
    }


# ── Export ──────────────────────────────────────────────────────────────────


def export_csv(G: nx.DiGraph, output_dir: Path) -> None:
    """Exporta nós e arestas em CSV."""
    output_dir.mkdir(parents=True, exist_ok=True)

    # graph_nodes.csv
    with open(output_dir / "graph_nodes.csv", "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["id", "kind", "file", "line"])
        for node, data in sorted(G.nodes(data=True)):
            writer.writerow([node, data.get("kind", ""), data.get("file", ""), data.get("line", "")])

    # graph_edges.csv
    with open(output_dir / "graph_edges.csv", "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["source", "target", "type", "weight", "line"])
        for u, v, data in sorted(G.edges(data=True)):
            writer.writerow([u, v, data.get("edge_type", ""), data.get("weight", 1), data.get("line", "")])


# ── CLI ─────────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Pipeline de extração de símbolos — Graphify Milestone 4"
    )
    parser.add_argument(
        "--target-dir", type=Path, required=True, help="Diretório para analisar"
    )
    parser.add_argument(
        "--output-dir", type=Path, default=Path("output"), help="Diretório de saída (default: ./output)"
    )
    args = parser.parse_args()

    if not args.target_dir.is_dir():
        log.error(f"Diretório não encontrado: {args.target_dir}")
        sys.exit(1)

    # Construir grafo
    G, stats = build_graph(args.target_dir)

    # Exportar CSV
    export_csv(G, args.output_dir)
    log.info(f"CSVs exportados em {args.output_dir}/")

    # Gerar e exportar relatório de qualidade
    report = quality_report(G, stats)
    report_path = args.output_dir / "quality_report.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    log.info(f"Relatório de qualidade: {report_path}")

    # Sumário
    print(f"\n{'='*60}")
    print(f"Pipeline de Símbolos — Sumário")
    print(f"{'='*60}")
    print(f"Arquivos:     {stats['total_files']}")
    print(f"Definições:   {stats['total_symbols']}")
    print(f"Chamadas:     {stats['total_calls']}")
    print(f"Nós no grafo: {G.number_of_nodes()}")
    print(f"Arestas:      {G.number_of_edges()}")
    print(f"Duplicatas:   {stats['duplicates_removed']} removidas")

    # Top 5 símbolos mais chamados (in-degree de arestas CALLS)
    call_edges = [(u, v) for u, v, d in G.edges(data=True) if d.get("edge_type") == "calls"]
    call_subgraph = G.edge_subgraph(call_edges) if call_edges else nx.DiGraph()

    if call_subgraph.number_of_edges() > 0:
        in_deg = sorted(call_subgraph.in_degree(), key=lambda x: x[1], reverse=True)[:10]
        print(f"\nTop 10 — Mais chamados (in-degree CALLS):")
        for node, degree in in_deg:
            kind = G.nodes[node].get("kind", "?")
            print(f"  {degree:>3}  [{kind:<8}] {node}")


if __name__ == "__main__":
    main()
