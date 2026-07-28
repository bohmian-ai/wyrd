"""Build small complete local bundles used by WyrdState hydration tests."""

from __future__ import annotations

import base64
import hashlib
from pathlib import Path
from typing import Any

import joblib
import yaml
from sklearn.linear_model import LogisticRegression
from wyrd.data import DataInterface, DataStats
from wyrd.model import ModelInterface


class TinyModelInterface(ModelInterface):
    """Test-only model loader that records its local path and load arguments."""

    def __init__(self) -> None:
        """Initialize empty model and preprocessor slots for hydration assertions."""
        super().__init__()
        self.model: Any = None
        self.preprocessor: Any = None
        self.processor: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        """Write deterministic bytes below ``path`` and return their statistics."""
        del save_kwargs
        output = path / "model" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = b"tiny-model"
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=hashlib.sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record the verified artifact directory and expose callable test objects."""
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.model = lambda value: value
        self.preprocessor = lambda value: value
        self.processor = None


class TinyDataInterface(DataInterface):
    """Test-only data loader that records local loading and returns a tiny dataset."""

    def __init__(self) -> None:
        """Initialize an empty data value and loader telemetry fields."""
        super().__init__()
        self.data: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        """Write deterministic bytes below ``path`` and return their statistics."""
        del save_kwargs
        output = path / "data" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = b"tiny-data"
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=hashlib.sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record the verified artifact directory and expose a deterministic data value."""
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.data = {"rows": [{"value": 1}]}


def _ref(kind: str, name: str, uid: str) -> dict[str, str]:
    """Return one complete CardRef mapping with stable fixture identity fields."""
    return {"kind": kind, "name": name, "version": "1.0.0", "space": "default", "uid": uid}


def _card(ref: dict[str, str], spec: dict[str, Any]) -> dict[str, Any]:
    """Wrap a fixture spec in the complete v1 Card envelope expected by WyrdState."""
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
        "relationships": {"outbound": [], "outbound_refs": [], "inbound": [], "inbound_refs": []},
    }


def build_complete_bundle(tmp_path: Path, *, duplicate_model_alias: bool = False) -> Path:
    """Create a complete offline service bundle with model, data, prompt, and agents.

    The builder writes only local YAML, alias indexes, inventories, and tiny artifact
    bytes. It never starts a server; tests intentionally exercise the real Rust loader.
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
        "publishes_to": [],
    }
    data_spec = {
        "interface": {
            "kind": "Custom",
            "meta": {"loader_module": "fixture", "loader_class": "TinyData", "extra": {}},
        },
        "schema": {"columns": []},
        "card_refs": [],
        "publishes_to": [],
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
                agent, {"prompt": prompt, "tool_names": [], "run_config": {}, "publishes_to": []}
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
                        {"alias": "training_data", "ref": data},
                        {"alias": "agent_triage", "ref": agent},
                        {"alias": "agent_inline", "ref": inline_agent},
                    ],
                    "publishes_to": [],
                },
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


def build_builtin_model_bundle(tmp_path: Path) -> Path:
    """Create a complete bundle whose primary Model uses native Sklearn loading.

    A fitted ``LogisticRegression`` is serialized as ``model.joblib`` beneath the
    verified model artifact directory; no custom interface override is needed.
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
