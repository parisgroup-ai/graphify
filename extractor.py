"""
extractor.py — Extração de símbolos (definições + chamadas) via tree-sitter.

Milestone 4: vai além de imports — extrai funções, classes e chamadas
com rastreabilidade (arquivo + linha) para cada símbolo.
"""

import tree_sitter_python as tspython
from tree_sitter import Language, Parser

PY_LANGUAGE = Language(tspython.language())


def create_parser() -> Parser:
    return Parser(PY_LANGUAGE)


# ── Navegação da AST ────────────────────────────────────────────────────────


def _walk(node):
    """Percorre todos os nós da AST (depth-first)."""
    yield node
    for child in node.children:
        yield from _walk(child)


# ── Passo 3: Extração de definições ────────────────────────────────────────


def extract_symbols(source: bytes, filepath: str, tree) -> list[dict]:
    """Extrai definições (funções e classes) com nome, tipo, arquivo e linha.

    Retorna lista de dicts: {name, kind, file, line}
    """
    symbols = []
    for node in _walk(tree.root_node):
        # Filtra só os nós que representam definições
        if node.type in ("function_definition", "class_definition"):
            # O nome vem do primeiro filho com tipo "identifier"
            for child in node.children:
                if child.type == "identifier":
                    symbols.append({
                        "name": child.text.decode(),
                        "kind": "function" if node.type == "function_definition" else "class",
                        "file": filepath,
                        "line": node.start_point[0] + 1,  # tree-sitter usa 0-indexed
                    })
                    break  # só precisa do primeiro identifier (o nome)
    return symbols


# ── Passo 4: Extração de chamadas ──────────────────────────────────────────


def extract_calls(source: bytes, filepath: str, tree) -> list[dict]:
    """Extrai chamadas de função com nome do callee, arquivo e linha.

    Trata dois casos:
      - Chamada simples: foo()        → node.type == "call", filho "identifier"
      - Chamada com atributo: obj.method() → filho "attribute", pega o método

    Retorna lista de dicts: {callee_name, file, line}
    """
    calls = []
    for node in _walk(tree.root_node):
        if node.type == "call":
            func_node = node.children[0] if node.children else None
            if func_node is None:
                continue

            callee_name = None

            if func_node.type == "identifier":
                # foo()
                callee_name = func_node.text.decode()

            elif func_node.type == "attribute":
                # obj.method() → pega "method" (último identifier)
                for child in func_node.children:
                    if child.type == "identifier":
                        callee_name = child.text.decode()
                # callee_name fica com o último identifier encontrado = nome do método

            if callee_name:
                calls.append({
                    "callee_name": callee_name,
                    "file": filepath,
                    "line": node.start_point[0] + 1,
                })
    return calls


# ── Teste rápido ────────────────────────────────────────────────────────────

if __name__ == "__main__":
    import sys
    from pathlib import Path

    # Uso: python extractor.py <arquivo.py>
    target = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__)
    source = target.read_bytes()

    parser = create_parser()
    tree = parser.parse(source)

    symbols = extract_symbols(source, str(target), tree)
    calls = extract_calls(source, str(target), tree)

    print(f"Arquivo: {target}")
    print(f"\n{'='*60}")
    print(f"Definições encontradas: {len(symbols)}")
    print(f"{'='*60}")
    for s in symbols:
        print(f"  {s['kind']:<10} {s['name']:<30} linha {s['line']}")

    print(f"\n{'='*60}")
    print(f"Chamadas encontradas: {len(calls)}")
    print(f"{'='*60}")
    for c in calls:
        print(f"  {c['callee_name']:<30} linha {c['line']}")
