"""
graphify extract — CLI para extrair imports via tree-sitter e gerar grafo JSON.

Uso:
    python graphify_extract.py --target-dir ./app --output graph.json

Requisitos:
    pip install networkx tree-sitter tree-sitter-python
"""

import argparse
import json
import logging
import sys
from pathlib import Path

import networkx as nx
import tree_sitter_python as tspython
from networkx.readwrite import json_graph
from tree_sitter import Language, Parser

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
log = logging.getLogger("graphify")

# ── Tree-sitter setup ────────────────────────────────────────────────────────

PY_LANGUAGE = Language(tspython.language())


def create_parser() -> Parser:
    return Parser(PY_LANGUAGE)


# ── Import extraction via AST ────────────────────────────────────────────────


def extract_imports_from_file(filepath: Path, parser: Parser) -> list[str]:
    """Parse a Python file and extract all imported module names via AST."""
    try:
        source = filepath.read_bytes()
    except (OSError, IOError) as e:
        log.warning(f"Cannot read {filepath}: {e}")
        return []

    try:
        tree = parser.parse(source)
    except Exception as e:
        log.warning(f"Parse error in {filepath}: {e}")
        return []

    imports: list[str] = []
    root = tree.root_node

    for node in _walk(root):
        if node.type == "import_statement":
            # import os / import app.services.llm
            for child in node.children:
                if child.type == "dotted_name":
                    imports.append(child.text.decode())

        elif node.type == "import_from_statement":
            # from app.services.llm import LLMClient
            module_name = _extract_from_module(node)
            if module_name:
                imports.append(module_name)

    return imports


def _walk(node):
    """Walk all nodes in the AST (breadth-first)."""
    yield node
    for child in node.children:
        yield from _walk(child)


def _extract_from_module(node) -> str | None:
    """Extract the module name from an import_from_statement node."""
    for child in node.children:
        if child.type == "dotted_name":
            return child.text.decode()
        if child.type == "relative_import":
            # Relative imports (from ..models import X)
            dots = ""
            module = ""
            for sub in child.children:
                if sub.type == "import_prefix":
                    dots = sub.text.decode()
                elif sub.type == "dotted_name":
                    module = sub.text.decode()
            return dots + module if dots or module else None
    return None


# ── File discovery ───────────────────────────────────────────────────────────


def find_python_files(target_dir: Path) -> list[Path]:
    """Find all .py files, excluding __pycache__, tests, migrations."""
    excluded = {"__pycache__", "node_modules", ".git", "dist", ".next"}
    result = []
    for p in sorted(target_dir.rglob("*.py")):
        parts = set(p.parts)
        if not parts & excluded:
            result.append(p)
    return result


# ── Path normalization ───────────────────────────────────────────────────────


def filepath_to_module(filepath: Path, target_dir: Path) -> str:
    """Convert a file path to a Python module name.

    app/services/v3/pipeline_orchestrator.py → app.services.v3.pipeline_orchestrator
    """
    relative = filepath.relative_to(target_dir.parent)
    parts = list(relative.with_suffix("").parts)
    # Remove __init__ — the module is the package
    if parts and parts[-1] == "__init__":
        parts = parts[:-1]
    return ".".join(parts)


# ── Graph construction ───────────────────────────────────────────────────────


def build_graph(target_dir: Path) -> nx.DiGraph:
    """Extract imports from all Python files and build a directed graph."""
    parser = create_parser()
    files = find_python_files(target_dir)

    log.info(f"Found {len(files)} Python files in {target_dir}")

    G = nx.DiGraph()
    real_modules: set[str] = set()

    # Register all real files as nodes
    for f in files:
        module = filepath_to_module(f, target_dir)
        real_modules.add(module)
        G.add_node(module, file_path=str(f.relative_to(target_dir.parent)), is_local=True)

    # Extract imports and create edges
    for f in files:
        source_module = filepath_to_module(f, target_dir)
        imported = extract_imports_from_file(f, parser)

        for target_module in imported:
            # Skip stdlib and relative imports with dots
            if target_module.startswith("."):
                continue

            # Add edge (DiGraph ignores duplicates automatically)
            if not G.has_node(target_module):
                G.add_node(target_module, file_path="", is_local=False)
            G.add_edge(source_module, target_module, kind="IMPORTS")

    # Add degree metrics to each node
    for node in G.nodes():
        G.nodes[node]["in_degree"] = G.in_degree(node)
        G.nodes[node]["out_degree"] = G.out_degree(node)

    return G


# ── Export ───────────────────────────────────────────────────────────────────


def export_json(G: nx.DiGraph, output_path: Path) -> None:
    """Export graph as JSON (node-link format)."""
    data = json_graph.node_link_data(G)
    with open(output_path, "w") as f:
        json.dump(data, f, indent=2, sort_keys=True)
    log.info(f"JSON exported: {output_path} ({G.number_of_nodes()} nodes, {G.number_of_edges()} edges)")


# ── CLI ──────────────────────────────────────────────────────────────────────


def main():
    parser = argparse.ArgumentParser(
        description="Graphify Extract — Build dependency graph from Python imports"
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        required=True,
        help="Directory to analyze (e.g., ./app)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("graph.json"),
        help="Output JSON path (default: graph.json)",
    )
    args = parser.parse_args()

    if not args.target_dir.is_dir():
        log.error(f"Target directory not found: {args.target_dir}")
        sys.exit(1)

    G = build_graph(args.target_dir)
    export_json(G, args.output)

    # Print summary
    print(f"\n{'='*60}")
    print(f"Graphify Extract — Summary")
    print(f"{'='*60}")
    print(f"Target:  {args.target_dir}")
    print(f"Nodes:   {G.number_of_nodes()}")
    print(f"Edges:   {G.number_of_edges()}")
    print(f"Output:  {args.output}")

    # Top hotspots
    in_deg = sorted(G.in_degree(), key=lambda x: x[1], reverse=True)[:5]
    out_deg = sorted(G.out_degree(), key=lambda x: x[1], reverse=True)[:5]

    print(f"\nTop 5 — In-degree (most depended on):")
    for node, degree in in_deg:
        marker = " ★" if G.nodes[node].get("is_local") else ""
        print(f"  {degree:>3}  {node}{marker}")

    print(f"\nTop 5 — Out-degree (most dependencies):")
    for node, degree in out_deg:
        marker = " ★" if G.nodes[node].get("is_local") else ""
        print(f"  {degree:>3}  {node}{marker}")


if __name__ == "__main__":
    main()
