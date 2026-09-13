"""End-to-end load + apply_defaults via the Python SDK."""

from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import CardKind
from wyrd.config import WyrdConfig


def test_load_round_trip(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n[defaults.labels]\nteam = "churn-ml"\n')
    cfg = WyrdConfig.load(cfg_file)
    assert "WyrdConfig" in repr(cfg)
    assert "space='prod'" in repr(cfg)


def test_load_explicit_missing_raises_typed(tmp_path: Path) -> None:
    missing = tmp_path / "no-such.toml"
    with pytest.raises(WyrdError) as exc:
        WyrdConfig.load(missing)
    assert exc.value.code == "WYRD_CFG_400_INVALID_TOML"


def test_apply_defaults_in_place(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn-classifier"}
    cfg.apply_defaults(meta, "Model")
    assert meta["space"] == "prod"
    assert meta["name"] == "churn-classifier"


def test_apply_defaults_card_value_wins(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn", "space": "eval"}
    cfg.apply_defaults(meta, "Model")
    assert meta["space"] == "eval"


def test_apply_defaults_unknown_kind_raises_typed(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    with pytest.raises(WyrdError) as exc:
        cfg.apply_defaults({"name": "churn"}, "NotAKind")
    assert exc.value.code == "WYRD_CFG_400_SCHEMA_MISMATCH"


def test_apply_defaults_missing_name_raises_typed(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    with pytest.raises(WyrdError) as exc:
        cfg.apply_defaults({"space": "x"}, "Model")
    assert exc.value.code == "WYRD_CFG_400_SCHEMA_MISMATCH"
    assert "name" in str(exc.value)


def test_apply_defaults_invalid_name_raises_typed(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    with pytest.raises(WyrdError) as exc:
        cfg.apply_defaults({"name": "INVALID"}, "Model")
    assert exc.value.code == "WYRD_CFG_400_SCHEMA_MISMATCH"


def test_apply_defaults_accepts_typed_card_kind_enum(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults]\nspace = "prod"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn"}
    cfg.apply_defaults(meta, CardKind.Model)
    assert meta["space"] == "prod"


def test_load_missing_path_returns_empty(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    (tmp_path / ".git").mkdir()
    monkeypatch.chdir(tmp_path)
    cfg = WyrdConfig.load(None)
    meta = {"name": "churn"}
    cfg.apply_defaults(meta, "Model")
    assert "space" not in meta


def test_apply_defaults_labels_are_merged(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults.labels]\nteam = "ml"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn-classifier"}
    cfg.apply_defaults(meta, "Model")
    assert meta["labels"]["team"] == "ml"


def test_apply_defaults_card_label_survives_merge(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults.labels]\nteam = "platform"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn", "labels": {"team": "ml-team", "domain": "customer"}}
    cfg.apply_defaults(meta, "Model")
    assert meta["labels"]["team"] == "ml-team"
    assert meta["labels"]["domain"] == "customer"


def test_apply_defaults_annotations_are_merged(tmp_path: Path) -> None:
    cfg_file = tmp_path / "wyrd.toml"
    cfg_file.write_text('[defaults.annotations]\nowner = "ml-platform"\n')
    cfg = WyrdConfig.load(cfg_file)
    meta = {"name": "churn-classifier"}
    cfg.apply_defaults(meta, "Model")
    assert meta["annotations"]["owner"] == "ml-platform"
