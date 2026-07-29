"""Build small complete local bundles used by WyrdState hydration tests."""

from __future__ import annotations

import base64
import hashlib
import json
from pathlib import Path
from typing import Any

import blake3
import joblib
import yaml
from sklearn.linear_model import LogisticRegression
from wyrd.data import DataInterface, DataStats
from wyrd.model import ModelInterface


class TinyModelInterface(ModelInterface):
    """Test-only Model interface for proving offline loader delegation.

    The fixture writes deterministic bytes below a caller-owned bundle path,
    records the artifact directory and kwargs passed by Rust, and exposes
    identity callables as loaded model and preprocessor values. It performs no
    network access; the ``model`` and ``preprocessor`` attributes are deliberate
    test invariants rather than production inference implementations.
    """

    def __init__(self) -> None:
        """Initialize empty holder slots and loader telemetry for assertions.

        The superclass is initialized first so the object remains a valid
        ``ModelInterface``; all additional fields are test-only observations.
        """
        super().__init__()
        self.model: Any = None
        self.preprocessor: Any = None
        self.processor: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        """Write deterministic fixture bytes below ``path`` and report them.

        Args:
            path: Artifact-root directory supplied by the hydration runtime.
            save_kwargs: Ignored test-only save options.
        Returns:
            Byte count and SHA-256 digest for the created payload.
        Side effects:
            Creates ``path/model/tiny.bin`` on the local filesystem.
        """
        del save_kwargs
        output = path / "model" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = b"tiny-model"
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=hashlib.sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record local load inputs and expose deterministic callable values.

        Args:
            path: Verified bundle-local artifact directory.
            load_kwargs: Alias-specific options forwarded by ``WyrdState``.
        Side effects:
            Reads no bytes and performs no network access; it only records the
            path/options and sets the test holder attributes.
        """
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.model = lambda value: value
        self.preprocessor = lambda value: value
        self.processor = None


class TinyDataInterface(DataInterface):
    """Test-only Data interface for proving local dataset hydration.

    Loading records the verified artifact directory and publishes a tiny
    deterministic mapping. The mapping is intentionally a test-only invariant
    used to prove the public DataCard holder remains alive after hydration.
    """

    def __init__(self) -> None:
        """Initialize empty data state and test-only loader telemetry.

        The constructor performs no filesystem or network work. Its fields are
        mutable observations used to verify that Rust hydration forwards the
        confined artifact path and preserves the loaded DataCard holder.
        """
        super().__init__()
        self.data: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        """Write deterministic fixture bytes below ``path`` and report them.

        Args:
            path: Artifact-root directory supplied by the hydration runtime.
            save_kwargs: Ignored test-only save options.
        Returns:
            Byte count and SHA-256 digest for the created payload.
        Side effects:
            Creates ``path/data/tiny.bin`` on the local filesystem.
        """
        del save_kwargs
        output = path / "data" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = b"tiny-data"
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=hashlib.sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record local load inputs and expose a deterministic data mapping.

        Args:
            path: Verified bundle-local artifact directory.
            load_kwargs: Alias-specific options forwarded by ``WyrdState``.
        Side effects:
            Performs no network access and does not read payload bytes; it only
            records arguments and sets the test dataset invariant.
        """
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.data = {"rows": [{"value": 1}]}


def _ref(kind: str, name: str, uid: str) -> dict[str, str]:
    """Build one deterministic CardRef mapping for a fixture Card.

    Args:
        kind: Native Card kind string.
        name: Friendly Card name.
        uid: Stable fixture UID.
    Returns:
        A complete versioned and space-qualified mapping accepted by the
        bundle loader. This helper is pure and has no filesystem side effects.
    """
    return {"kind": kind, "name": name, "version": "1.0.0", "space": "default", "uid": uid}


def _ref_string(ref: dict[str, str]) -> str:
    """Render one fixture CardRef using the canonical Wyrd identity string."""
    return f"{ref['space']}/{ref['kind']}/{ref['name']}@{ref['version']}#{ref['uid']}"


def _relationships(
    *entries: tuple[dict[str, str], str | None],
) -> dict[str, list[Any]]:
    """Project exact outbound references and optional Service aliases."""
    ordered = sorted(entries, key=lambda entry: _ref_string(entry[0]))
    return {
        "outbound": [_ref_string(ref) for ref, _alias in ordered],
        "outbound_refs": [{"ref": ref, "alias": alias} for ref, alias in ordered],
        "inbound": [],
        "inbound_refs": [],
    }


def _card(
    ref: dict[str, str],
    spec: dict[str, Any],
    relationships: dict[str, list[Any]] | None = None,
) -> dict[str, Any]:
    """Wrap a fixture spec in the complete ``wyrd/v1`` Card envelope.

    Args:
        ref: CardRef mapping produced by :func:`_ref`.
        spec: Kind-specific declarative spec mapping.
    Returns:
        A JSON/YAML-compatible complete Card dictionary with empty relationship
        lists. The conversion is pure and does not write files.
    """
    return {
        "apiVersion": "wyrd/v1",
        "kind": ref["kind"],
        "metadata": {
            "space": ref["space"],
            "name": ref["name"],
            "version": ref["version"],
            "uid": ref["uid"],
            "labels": {},
            "annotations": {},
        },
        "spec": spec,
        "relationships": relationships or _relationships(),
    }


def build_complete_bundle(tmp_path: Path, *, duplicate_model_alias: bool = False) -> Path:
    """Create a complete offline service bundle with model, data, prompt, and agents.

    Args:
        tmp_path: Pytest-managed temporary filesystem root.
        duplicate_model_alias: Add ``primary_model`` pointing at the same exact
            Model Card to exercise persistent identity and config conflict rules.
    Returns:
        The complete ``bundle`` directory containing metadata, Card YAML,
        relationships, aliases, inventories, and confined payloads.
    Side effects:
        Creates and writes the complete fixture tree below ``tmp_path``. It
        never starts a server or contacts a registry.
    """
    root = tmp_path / "bundle"
    root.mkdir()
    model = _ref("Model", "model", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b02")
    backup = _ref("Model", "backup", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b03")
    data = _ref("Data", "training", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b07")
    prompt = _ref("Prompt", "triage", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b04")
    agent = _ref("Agent", "triage", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b06")
    inline_agent = _ref("Agent", "inline", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b05")
    service = _ref("Service", "service", "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01")
    model_spec = {
        "interface": {
            "kind": "Custom",
            "meta": {
                "framework_version": "1",
                "loader_module": "fixture",
                "loader_class": "TinyModel",
                "extra": {},
            },
        },
        "task_type": "Other",
        "signature": {
            "inputs": [{"name": "input", "dtype": "float64"}],
            "outputs": [{"name": "output", "dtype": "float64"}],
        },
        "card_refs": [],
    }
    data_spec = {
        "interface": {
            "kind": "Custom",
            "meta": {"loader_module": "fixture", "loader_class": "TinyData", "extra": {}},
        },
        "schema": {"columns": []},
        "card_refs": [],
        "splits": {},
        "target_columns": [],
        "stats": {"row_count": None, "col_count": None, "byte_count": 0, "sha256": "0" * 64},
    }
    prompt_spec = {
        "provider": "openai",
        "model": "gpt-4o",
        "messages": "hello",
    }
    cards = {
        "model": (
            _card(model, {**model_spec}),
            ["model", "primary_model"] if duplicate_model_alias else ["model"],
        ),
        "backup": (_card(backup, {**model_spec}), ["backup"]),
        "training": (_card(data, data_spec), ["training_data"]),
        "triage": (_card(prompt, prompt_spec), ["triage_prompt"]),
        "triage_agent": (
            _card(
                agent,
                {"prompt": prompt, "tool_names": [], "run_config": {}, "publishes_to": []},
                _relationships((prompt, None)),
            ),
            ["agent_triage"],
        ),
        "inline_agent": (
            _card(
                inline_agent,
                {
                    "prompt": {
                        "request": {
                            "model": "gpt-4o",
                            "messages": [{"role": "user", "content": "inline"}],
                        },
                        "model": "gpt-4o",
                        "variables": [],
                        "media_variables": [],
                        "response_type": "text",
                    },
                    "tool_names": [],
                    "run_config": {},
                    "publishes_to": [],
                },
            ),
            ["agent_inline"],
        ),
        "service": (
            _card(
                service,
                {
                    "components": [
                        {"alias": "model", "ref": model},
                        {"alias": "backup", "ref": backup},
                        {"alias": "training_data", "ref": data},
                        {"alias": "agent_triage", "ref": agent},
                        {"alias": "agent_inline", "ref": inline_agent},
                    ],
                    "publishes_to": [],
                },
                _relationships(
                    (model, "model"),
                    (backup, "backup"),
                    (data, "training_data"),
                    (agent, "agent_triage"),
                    (inline_agent, "agent_inline"),
                ),
            ),
            ["root"],
        ),
    }
    manifest_cards = []
    for directory, (card, aliases) in cards.items():
        card_path = f"cards/{directory}/card.yaml"
        inventory_path = f"cards/{directory}/artifacts.yaml"
        artifact = []
        if directory in {"model", "backup", "training"}:
            payload = b"tiny-model" if directory in {"model", "backup"} else b"tiny-data"
            relative = "tiny.bin"
            local = f"cards/{directory}/artifacts/{relative}"
            artifact = [
                {
                    "relative_path": relative,
                    "sha256": base64.b64encode(hashlib.sha256(payload).digest()).decode(),
                    "size_bytes": len(payload),
                    "content_type": "application/octet-stream",
                    "local_path": local,
                }
            ]
            artifact_path = root / local
            artifact_path.parent.mkdir(parents=True, exist_ok=True)
            artifact_path.write_bytes(payload)
        card_file = root / card_path
        card_file.parent.mkdir(parents=True, exist_ok=True)
        card_file.write_text(yaml.safe_dump(card, sort_keys=False), encoding="utf-8")
        (root / inventory_path).write_text(
            yaml.safe_dump(artifact, sort_keys=False), encoding="utf-8"
        )
        for alias in aliases:
            alias_file = root / "aliases" / f"{alias}.yaml"
            alias_file.parent.mkdir(parents=True, exist_ok=True)
            alias_file.write_text(
                yaml.safe_dump(
                    {
                        "alias": alias,
                        "card_ref": {
                            k: card["metadata"][k] for k in ("name", "version", "space", "uid")
                        }
                        | {"kind": card["kind"]},
                        "card_path": card_path,
                    },
                    sort_keys=False,
                ),
                encoding="utf-8",
            )
        ref = {
            "kind": card["kind"],
            "name": card["metadata"]["name"],
            "version": card["metadata"]["version"],
            "space": card["metadata"]["space"],
            "uid": card["metadata"]["uid"],
        }
        manifest_cards.append(
            {
                "aliases": aliases,
                "card_ref": ref,
                "card_path": card_path,
                "relationships_path": f"cards/{directory}/relationships.yaml",
                "artifact_inventory_path": inventory_path,
                "artifacts": artifact,
            }
        )
        (root / f"cards/{directory}/relationships.yaml").write_text(
            yaml.safe_dump(card["relationships"], sort_keys=False), encoding="utf-8"
        )
    manifest = {
        "apiVersion": "wyrd/hydrated-bundle/v1",
        "hydration": "complete",
        "root": service,
        "cards": manifest_cards,
        "card_count": len(manifest_cards),
        "artifact_count": 3,
        "downloaded_artifact_count": 3,
    }
    (root / "metadata.yaml").write_text(yaml.safe_dump(manifest, sort_keys=False), encoding="utf-8")
    return root


def trusted_artifact_hash(bundle: Path, alias: str) -> str:
    """Compute the exact canonical artifact-manifest hash trusted by WyrdState."""
    manifest = yaml.safe_load((bundle / "metadata.yaml").read_text(encoding="utf-8"))
    selected = next(card for card in manifest["cards"] if alias in card["aliases"])
    artifacts = [
        {
            "relative_path": artifact["relative_path"],
            "sha256": artifact["sha256"],
            "size_bytes": artifact["size_bytes"],
            "content_type": artifact.get("content_type"),
        }
        for artifact in selected["artifacts"]
    ]
    artifacts.sort(key=lambda artifact: artifact["relative_path"])
    canonical = json.dumps(
        artifacts,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    ).encode()
    return blake3.blake3(canonical).hexdigest()


def build_builtin_model_bundle(tmp_path: Path) -> Path:
    """Create a complete bundle whose primary Model uses native Sklearn loading.

    Args:
        tmp_path: Pytest-managed temporary filesystem root.
    Returns:
        The complete bundle directory containing a fitted ``model.joblib``.
    Side effects:
        Rewrites the model Card and metadata, removes the tiny custom payload,
        and writes joblib bytes below the exact model artifact root. The
        test-only invariant is that no ``model`` interface override is needed.
    """
    root = build_complete_bundle(tmp_path)
    card_path = root / "cards/model/card.yaml"
    card = yaml.safe_load(card_path.read_text(encoding="utf-8"))
    card["spec"]["interface"] = {
        "kind": "Sklearn",
        "meta": {"framework_version": "1.4.0", "model_subtype": "LogisticRegression"},
    }
    card_path.write_text(yaml.safe_dump(card, sort_keys=False), encoding="utf-8")
    old = root / "cards/model/artifacts/tiny.bin"
    old.unlink()
    target = root / "cards/model/artifacts/model.joblib"
    model = LogisticRegression().fit([[0.0], [1.0]], [0, 1])
    joblib.dump(model, target)
    payload = target.read_bytes()
    inventory = [
        {
            "relative_path": "model.joblib",
            "sha256": base64.b64encode(hashlib.sha256(payload).digest()).decode(),
            "size_bytes": len(payload),
            "content_type": "application/octet-stream",
            "local_path": "cards/model/artifacts/model.joblib",
        }
    ]
    (root / "cards/model/artifacts.yaml").write_text(
        yaml.safe_dump(inventory, sort_keys=False), encoding="utf-8"
    )
    manifest = yaml.safe_load((root / "metadata.yaml").read_text(encoding="utf-8"))
    manifest["cards"][0]["artifacts"] = inventory
    (root / "metadata.yaml").write_text(yaml.safe_dump(manifest, sort_keys=False), encoding="utf-8")
    return root
