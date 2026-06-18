"""Public-surface check for wyrd.config."""

import wyrd
import wyrd.config as config_module


def test_wyrd_config_all_exports_are_importable() -> None:
    for name in config_module.__all__:
        assert getattr(config_module, name) is not None


def test_wyrd_config_is_re_exported_at_package_root() -> None:
    assert hasattr(wyrd, "WyrdConfig")
