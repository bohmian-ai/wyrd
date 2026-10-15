"""Check the hand-authored Wyrd stubs without writing any file.

The package-local ``.pyi`` files are the public typing source for the native
extension, and the private pure-Python modules they re-export supply the rest.
This check enforces the conventions editors and type checkers rely on:
constructors are documented as ``__init__``, every public member has a
docstring with complete ``Args:``, no stub declares a class twice, each
package's stub exports the same ``__all__`` as its runtime module, and no
stray root module shadows a public package.

Run from the package root: ``uv run python scripts/check_stubs.py``. Exits
nonzero and lists every failure when any convention is broken.
"""

from __future__ import annotations

import ast
import re
import sys
from pathlib import Path

PACKAGE_DIR = Path("python/wyrd")
ROOT_STUBS = {"_wyrd.pyi", "__init__.pyi"}
FUNCTION_NODES = (ast.FunctionDef, ast.AsyncFunctionDef)
ARGS_ENTRY = re.compile(r"^\s+\**(\w+)(?:\s*\([^)]*\))?\s*:")


def undocumented_public_members(tree: ast.Module) -> list[str]:
    """Return public members whose docstring is missing or incomplete.

    Stubs are the help text editors show, so every public class, ``__init__``,
    public method, and module function needs a docstring, and every
    parameter other than ``self`` or ``cls`` needs one ``Args:`` entry.
    ``@overload`` variants of one name are checked together: each parameter
    of any variant must be documented by some variant's docstring.

    Args:
        tree: The parsed module or stub source.

    Returns:
        ``name`` for a member without a docstring and ``name(param)`` for a
        parameter without an ``Args:`` entry, in source order.
    """
    missing: list[str] = []
    module_functions = [node for node in tree.body if isinstance(node, FUNCTION_NODES)]
    missing.extend(undocumented_callables(module_functions, prefix="", is_method=False))
    for node in tree.body:
        if not isinstance(node, ast.ClassDef) or node.name.startswith("_"):
            continue
        if ast.get_docstring(node) is None:
            missing.append(node.name)
        methods = [child for child in node.body if isinstance(child, FUNCTION_NODES)]
        missing.extend(undocumented_callables(methods, prefix=f"{node.name}.", is_method=True))
    return missing


def undocumented_callables(
    functions: list[ast.FunctionDef | ast.AsyncFunctionDef], *, prefix: str, is_method: bool
) -> list[str]:
    """Check one scope's public callables, grouping ``@overload`` variants by name.

    Args:
        functions: The scope's function definitions in source order.
        prefix: ``"Class."`` for methods, empty for module functions.
        is_method: Whether a leading ``self`` or ``cls`` parameter is implicit.

    Returns:
        The missing docstrings and ``Args:`` entries, as for
        ``undocumented_public_members``.
    """
    groups: dict[str, list[ast.FunctionDef | ast.AsyncFunctionDef]] = {}
    for function in functions:
        if function.name != "__init__" and function.name.startswith("_"):
            continue
        groups.setdefault(function.name, []).append(function)
    missing: list[str] = []
    for name, variants in groups.items():
        docstrings = [doc for doc in map(ast.get_docstring, variants) if doc is not None]
        if not docstrings:
            missing.append(f"{prefix}{name}")
            continue
        documented = set().union(*map(documented_args, docstrings))
        parameters: list[str] = []
        for variant in variants:
            for parameter in signature_parameters(variant, is_method=is_method):
                if parameter not in parameters:
                    parameters.append(parameter)
        missing.extend(
            f"{prefix}{name}({parameter})"
            for parameter in parameters
            if parameter not in documented
        )
    return missing


def signature_parameters(
    function: ast.FunctionDef | ast.AsyncFunctionDef, *, is_method: bool
) -> list[str]:
    """Return a callable's parameter names, without an implicit ``self`` or ``cls``.

    Args:
        function: The function definition.
        is_method: Whether the callable is defined in a class body.

    Returns:
        Positional, ``*args``, keyword-only, and ``**kwargs`` names in order.
    """
    arguments = function.args
    names = [argument.arg for argument in [*arguments.posonlyargs, *arguments.args]]
    is_static = any(getattr(d, "id", None) == "staticmethod" for d in function.decorator_list)
    if is_method and not is_static and names and names[0] in {"self", "cls"}:
        names = names[1:]
    if arguments.vararg is not None:
        names.append(arguments.vararg.arg)
    names.extend(argument.arg for argument in arguments.kwonlyargs)
    if arguments.kwarg is not None:
        names.append(arguments.kwarg.arg)
    return names


def documented_args(docstring: str) -> set[str]:
    """Return the parameter names one Google-style ``Args:`` section documents.

    Args:
        docstring: The cleaned docstring.

    Returns:
        Each entry's name; empty when the docstring has no ``Args:`` section.
    """
    names: set[str] = set()
    in_args = False
    entry_indent: int | None = None
    for line in docstring.splitlines():
        if line.strip() == "Args:":
            in_args, entry_indent = True, None
            continue
        if not in_args or not line.strip():
            continue
        indent = len(line) - len(line.lstrip())
        if indent == 0:
            in_args = False
            continue
        entry_indent = indent if entry_indent is None else entry_indent
        match = ARGS_ENTRY.match(line)
        if indent == entry_indent and match is not None:
            names.add(match.group(1))
    return names


def constructor_errors(path: Path, tree: ast.Module) -> list[str]:
    """Return a failure for each ``__new__`` a public stub declares.

    Args:
        path: The stub file, for the message.
        tree: The parsed stub.

    Returns:
        One message per ``__new__`` definition.
    """
    return [
        f"{path}:{node.lineno}: document constructors as __init__, not __new__"
        for node in ast.walk(tree)
        if isinstance(node, FUNCTION_NODES) and node.name == "__new__"
    ]


def duplicate_class_errors(path: Path, tree: ast.Module) -> list[str]:
    """Return a failure for each class name one stub declares more than once.

    Args:
        path: The stub file, for the message.
        tree: The parsed stub.

    Returns:
        One message per repeated declaration, naming both lines.
    """
    seen: dict[str, int] = {}
    errors: list[str] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.ClassDef):
            continue
        if node.name in seen:
            errors.append(
                f"{path}: class {node.name} declared at lines {seen[node.name]} and {node.lineno}"
            )
        else:
            seen[node.name] = node.lineno
    return errors


def reexported_private_modules(path: Path, tree: ast.Module) -> list[Path]:
    """Return the private pure-Python modules a stub re-exports from.

    Their definitions are public API reached through the stub, so they meet
    the same documentation rules.

    Args:
        path: The stub file.
        tree: The parsed stub.

    Returns:
        Each existing sibling ``_name.py`` imported with ``from ._name import``.
    """
    modules: list[Path] = []
    for node in tree.body:
        if isinstance(node, ast.ImportFrom) and node.level == 1 and node.module:
            candidate = path.parent / f"{node.module}.py"
            if node.module.startswith("_") and candidate.exists() and candidate not in modules:
                modules.append(candidate)
    return modules


def literal_all(path: Path, tree: ast.Module) -> set[str] | None:
    """Return the names a module's literal ``__all__`` assignment lists.

    Args:
        path: The module file, for the message.
        tree: The parsed module.

    Returns:
        The listed names, or ``None`` when the module assigns no ``__all__``.

    Raises:
        SystemExit: When ``__all__`` is not a literal list of strings.
    """
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets
        ):
            try:
                return set(ast.literal_eval(node.value))
            except ValueError as error:
                raise SystemExit(f"{path}: __all__ must be a literal list") from error
    return None


def export_errors(stub: Path, stub_tree: ast.Module) -> list[str]:
    """Return a failure when a package stub and its runtime module export different names.

    Args:
        stub: The package ``__init__.pyi``.
        stub_tree: The parsed stub.

    Returns:
        One message naming the names only one side exports, or none.
    """
    runtime = stub.with_suffix(".py")
    if stub.name != "__init__.pyi" or not runtime.exists():
        return []
    runtime_all = literal_all(runtime, ast.parse(runtime.read_text(encoding="utf-8")))
    stub_all = literal_all(stub, stub_tree)
    if runtime_all == stub_all:
        return []
    runtime_all, stub_all = runtime_all or set(), stub_all or set()
    return [
        f"{stub}: __all__ differs from {runtime}: only in stub "
        f"{sorted(stub_all - runtime_all)}, only at runtime {sorted(runtime_all - stub_all)}"
    ]


def layout_errors(stubs: list[Path]) -> list[str]:
    """Return a failure for a stray root stub or a root module shadowing a package.

    Args:
        stubs: Every stub under the package.

    Returns:
        One message listing the stray files, or none.
    """
    stray = sorted(path.name for path in PACKAGE_DIR.glob("*.pyi") if path.name not in ROOT_STUBS)
    stray += sorted(
        f"{stub.parent.name}.py"
        for stub in stubs
        if stub.parent != PACKAGE_DIR and (PACKAGE_DIR / f"{stub.parent.name}.py").exists()
    )
    return [f"{PACKAGE_DIR}: unexpected root modules {stray}"] if stray else []


def check() -> list[str]:
    """Run every stub check over the package.

    Returns:
        Every failure, in file order; empty when the stubs are valid.
    """
    stubs = sorted(PACKAGE_DIR.rglob("*.pyi"))
    errors = layout_errors(stubs)
    documented: list[Path] = []
    for stub in stubs:
        tree = ast.parse(stub.read_text(encoding="utf-8"))
        errors += constructor_errors(stub, tree)
        errors += duplicate_class_errors(stub, tree)
        errors += export_errors(stub, tree)
        documented += [stub, *reexported_private_modules(stub, tree)]
    for path in dict.fromkeys(documented):
        missing = undocumented_public_members(ast.parse(path.read_text(encoding="utf-8")))
        if missing:
            errors.append(f"{path}: missing docstrings or Args entries: {', '.join(missing)}")
    return errors


if __name__ == "__main__":
    failures = check()
    for failure in failures:
        print(failure, file=sys.stderr)
    sys.exit(1 if failures else 0)
