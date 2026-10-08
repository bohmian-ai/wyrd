"""Assemble hand-authored Wyrd stub sources into public package stubs."""

from __future__ import annotations

import ast
import re
from pathlib import Path

STUB_DIR = Path("python/wyrd/stubs")
PACKAGE_DIR = Path("python/wyrd")
ROOT_OUTPUT_FILE = PACKAGE_DIR / "_wyrd.pyi"

PUBLIC_MODULE_STUBS = {
    "agent.pyi": PACKAGE_DIR / "agent" / "__init__.pyi",
    "bifrost.pyi": PACKAGE_DIR / "bifrost" / "__init__.pyi",
    "cards.pyi": PACKAGE_DIR / "cards" / "__init__.pyi",
    "cli.pyi": PACKAGE_DIR / "cli" / "__init__.pyi",
    "client.pyi": PACKAGE_DIR / "client" / "__init__.pyi",
    "config.pyi": PACKAGE_DIR / "config" / "__init__.pyi",
    "data.pyi": PACKAGE_DIR / "data" / "__init__.pyi",
    "eval.pyi": PACKAGE_DIR / "eval" / "__init__.pyi",
    "gateway.pyi": PACKAGE_DIR / "gateway" / "__init__.pyi",
    "model.pyi": PACKAGE_DIR / "model" / "__init__.pyi",
    "observe.pyi": PACKAGE_DIR / "observe" / "__init__.pyi",
    "operators.pyi": PACKAGE_DIR / "operators" / "__init__.pyi",
    "prompt.pyi": PACKAGE_DIR / "prompt" / "__init__.pyi",
    "state.pyi": PACKAGE_DIR / "state" / "__init__.pyi",
    "testing.pyi": PACKAGE_DIR / "testing" / "__init__.pyi",
    "testing_cli.pyi": PACKAGE_DIR / "testing" / "cli" / "__init__.pyi",
}

ROOT_STUB_FILES = ["header.pyi", "error.pyi"]
PACKAGE_STUB_FILE = "package.pyi"


def validate_source_stub(filename: str, raw_text: str) -> None:
    """Validate hand-authored stub conventions before assembly."""
    if re.search(r"^\s*def\s+__new__\s*\(", raw_text, flags=re.MULTILINE):
        raise SystemExit(
            f"{STUB_DIR / filename}: public stubs must document constructors as "
            "__init__, not __new__"
        )
    missing = undocumented_public_members(ast.parse(raw_text))
    if missing:
        raise SystemExit(
            f"{STUB_DIR / filename}: missing docstrings or Args entries: {', '.join(missing)}"
        )


def undocumented_public_members(tree: ast.Module) -> list[str]:
    """Return public stub members whose docstring is missing or incomplete.

    Stubs are the help text editors show, so every public class, ``__init__``,
    public method, and module function needs a docstring, and every
    parameter other than ``self`` or ``cls`` needs one ``Args:`` entry.
    ``@overload`` variants of one name are checked together: each parameter
    of any variant must be documented by some variant's docstring.

    Args:
        tree: The parsed stub source.

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


FUNCTION_NODES = (ast.FunctionDef, ast.AsyncFunctionDef)


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


ARGS_ENTRY = re.compile(r"^\s+\**(\w+)(?:\s*\([^)]*\))?\s*:")


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


def literal_all(tree: ast.Module) -> set[str]:
    """Return the names a module's literal ``__all__`` assignment lists."""
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets
        ):
            return set(ast.literal_eval(node.value))
    raise SystemExit("__all__ must be a literal list")


def validate_package_exports(stub_text: str) -> None:
    """Fail when the package stub and runtime ``__init__.py`` export different names."""
    runtime = literal_all(ast.parse((PACKAGE_DIR / "__init__.py").read_text(encoding="utf-8")))
    stub = literal_all(ast.parse(stub_text))
    if runtime != stub:
        raise SystemExit(
            f"{STUB_DIR / PACKAGE_STUB_FILE}: __all__ differs from {PACKAGE_DIR / '__init__.py'}: "
            f"only in stub {sorted(stub - runtime)}, only at runtime {sorted(runtime - stub)}"
        )


def validate_package_layout() -> None:
    """Fail when a stray root module or stub would shadow a public module package."""
    expected = {ROOT_OUTPUT_FILE.name, "__init__.pyi"}
    stray = sorted(path.name for path in PACKAGE_DIR.glob("*.pyi") if path.name not in expected)
    stray += sorted(
        f"{output.parent.name}.py"
        for output in PUBLIC_MODULE_STUBS.values()
        if (PACKAGE_DIR / f"{output.parent.name}.py").exists()
    )
    if stray:
        raise SystemExit(f"{PACKAGE_DIR}: unexpected root stubs {stray}")


def strip_imports_section(content: str) -> str:
    """Remove a module-local import block from a source stub."""
    pattern = r"####\s*begin\s+imports\s*####.*?####\s*end\s+of\s+imports\s*####\s*\n?"
    return re.sub(pattern, "", content, flags=re.DOTALL)


def remove_all_block(content: str) -> tuple[str, list[str]]:
    """Remove and return a source stub `__all__` block."""
    all_pattern = r"__all__\s*=\s*\[(.*?)\]"
    match = re.search(all_pattern, content, flags=re.DOTALL)
    if not match:
        return content, []
    items = re.findall(r'"([^"]+)"|\'([^\']+)\'', match.group(1))
    return re.sub(all_pattern, "", content, flags=re.DOTALL), [a or b for a, b in items]


def validate_no_duplicate_classes(path: Path, content: str) -> None:
    """Fail when one generated stub defines the same class name twice."""
    seen: dict[str, int] = {}
    duplicates: list[str] = []
    for line_no, line in enumerate(content.splitlines(), start=1):
        match = re.match(r"\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\b", line)
        if not match:
            continue
        name = match.group(1)
        if name in seen:
            duplicates.append(f"{name} at lines {seen[name]} and {line_no}")
        else:
            seen[name] = line_no
    if duplicates:
        joined = "; ".join(duplicates)
        raise SystemExit(f"{path}: duplicate class declarations: {joined}")


def write_stub(path: Path, lines: list[str]) -> None:
    """Write a generated stub after duplicate-class validation."""
    content = "\n".join(lines).strip() + "\n"
    validate_no_duplicate_classes(path, content)
    path.write_text(content, encoding="utf-8")


def source_text(filename: str) -> str:
    """Read and validate one hand-authored source stub."""
    path = STUB_DIR / filename
    raw_text = path.read_text(encoding="utf-8")
    validate_source_stub(filename, raw_text)
    return raw_text


def rewrite_public_imports(filename: str, content: str) -> str:
    """Rewrite source-stub imports for generated package-local stubs."""
    replacements = {
        "config.pyi": {
            "from .cards import CardKind": "from ..cards import CardKind",
        },
        "cards.pyi": {
            "from .data import DataCard, DataInterface\nfrom .model import ModelCard, ModelInterface\nfrom .prompt import PromptCard, PromptReference": (
                "from ..data import DataCard, DataInterface\n"
                "from ..model import ModelCard, ModelInterface\n"
                "from ..prompt import PromptCard, PromptReference"
            ),
        },
        "agent.pyi": {
            "from collections.abc import Callable, Mapping, Sequence": (
                "from collections.abc import Callable, Mapping, Sequence"
            ),
            "from .error import WyrdError\nfrom .header import JsonDict, PathLike": (
                "from .._wyrd import JsonDict, PathLike, WyrdError"
            ),
            "from .prompt import Prompt": "from ..prompt import Prompt",
            "from .cards import AgentCard": "from ..cards import AgentCard",
            "from .client import WyrdClient": "from ..client import WyrdClient",
        },
        "testing_cli.pyi": {
            "from .cards import CardRef, HydrationSummary, RegistrationReceipt": (
                "from ...cards import CardRef, HydrationSummary, RegistrationReceipt"
            ),
            "from .client import WyrdClient": "from ...client import WyrdClient",
            "from .gateway import ProviderCredentialView": (
                "from ...gateway import ProviderCredentialView"
            ),
        },
        "observe.pyi": {
            "from .bifrost import Bifrost": "from ..bifrost import Bifrost",
            "from .cards import CardRef": "from ..cards import CardRef",
            "from .eval import MediaRef": "from ..eval import MediaRef",
        },
        "data.pyi": {
            "from .cards import CardRef, DataLoadArgs, JsonValue\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError"
                "\nfrom ..cards import CardRef, DataLoadArgs, JsonValue"
            ),
            "from .cards import CardRef, DataLoadArgs\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError"
                "\nfrom ..cards import CardRef, DataLoadArgs"
            ),
            "from .cards import CardRef\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError"
                "\nfrom ..cards import CardRef"
            ),
        },
        "model.pyi": {
            "from .cards import CardRef, JsonValue, ModelLoadArgs\nfrom .data import FieldSpec\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError\n"
                "from ..cards import CardRef, JsonValue, ModelLoadArgs\n"
                "from ..data import FieldSpec"
            ),
            "from .cards import CardRef, ModelLoadArgs\nfrom .data import FieldSpec\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError\n"
                "from ..cards import CardRef, ModelLoadArgs\n"
                "from ..data import FieldSpec"
            ),
            "from .cards import CardRef\nfrom .data import FieldSpec\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError\n"
                "from ..cards import CardRef\n"
                "from ..data import FieldSpec"
            ),
        },
        "prompt.pyi": {
            "from .cards import CardRef\nfrom .error import WyrdError\nfrom .header import JsonDict, PathLike": (
                "from .._wyrd import JsonDict, PathLike, WyrdError\nfrom ..cards import CardRef"
            ),
        },
    }
    for before, after in replacements.get(filename, {}).items():
        content = content.replace(before, after)
    return content


def assemble_root_stub() -> None:
    """Write the root native extension stub."""
    final_content = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
    ]
    master_all: list[str] = []

    for filename in ROOT_STUB_FILES:
        raw_text = source_text(filename)
        if filename != "header.pyi":
            raw_text = strip_imports_section(raw_text)
        text_to_append, all_items = remove_all_block(raw_text)
        master_all.extend(all_items)

        final_content.append(f"### {filename} ###")
        final_content.append(text_to_append.strip())
        final_content.append("")

    final_content.append("### GLOBAL EXPORTS ###")
    final_content.append("__all__ = [")
    for item in sorted(set(master_all)):
        final_content.append(f'    "{item}",')
    final_content.append("]")

    write_stub(ROOT_OUTPUT_FILE, final_content)
    print(f"Compiled {len(master_all)} root exports into {ROOT_OUTPUT_FILE}")


def assemble_public_module_stubs() -> None:
    """Write package-local stubs for public Wyrd modules."""
    for filename, output_path in PUBLIC_MODULE_STUBS.items():
        raw_text = rewrite_public_imports(filename, source_text(filename))
        lines = [
            "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
            "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
            raw_text.strip(),
        ]
        write_stub(output_path, lines)
        print(f"Compiled {filename} into {output_path}")


def assemble_package_stub() -> None:
    """Write the root public package stub."""
    raw_text = source_text(PACKAGE_STUB_FILE)
    validate_package_exports(raw_text)
    lines = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
        raw_text.strip(),
    ]
    output_path = PACKAGE_DIR / "__init__.pyi"
    write_stub(output_path, lines)
    print(f"Compiled {PACKAGE_STUB_FILE} into {output_path}")


def assemble() -> None:
    """Write all generated public stubs."""
    assemble_root_stub()
    assemble_package_stub()
    assemble_public_module_stubs()
    validate_package_layout()


if __name__ == "__main__":
    assemble()
