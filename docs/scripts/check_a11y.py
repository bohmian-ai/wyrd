#!/usr/bin/env python3
"""Build-time a11y gate over prerendered docs/build HTML.

Asserts:
- <main> / <nav aria-label> landmarks on every real doc page
- single <h1> + non-skipping heading order
- accessible-name + focusability on search / theme-toggle / nav-drawer controls
- light+dark WCAG AA contrast from wyrd-tokens.css (text+muted on bg)

Excludes: docs/build/404.html (JS-only fallback shell) and docs/build/mocks/**
(component fixture pages). Requires: `mise run docs:build` first.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
BUILD_DIR = DOCS_ROOT / "build"
TOKENS_CSS = DOCS_ROOT / "src" / "styles" / "wyrd-tokens.css"
WCAG_AA = 4.5

# Fail-closed floor on a truncated prerender. The site emits 33 real doc content
# pages (the home and Fathom splashes are full-bleed archetypes, exempt from the
# doc-shell contract below). A masked prerender failure (a content page throwing
# at render) silently drops pages and omits index.html while the build still exits
# 0. Asserting index.html plus a page floor turns that regression class into a hard
# a11y-gate failure instead of a vacuously-green run over whatever HTML survived.
MIN_REAL_PAGES = 33


# ─── WCAG contrast helpers ────────────────────────────────────────────────────

def _linearise(c: int) -> float:
    s = c / 255.0
    return s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4


def _luminance(hex_color: str) -> float:
    h = hex_color.lstrip("#")
    if len(h) == 3:
        h = "".join(ch * 2 for ch in h)
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    return 0.2126 * _linearise(r) + 0.7152 * _linearise(g) + 0.0722 * _linearise(b)


def contrast_ratio(c1: str, c2: str) -> float:
    l1, l2 = _luminance(c1), _luminance(c2)
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


# ─── Token parsing ────────────────────────────────────────────────────────────

_HEX = re.compile(r'--([\w-]+)\s*:\s*(#[0-9a-fA-F]{3,6})')


def _parse_theme_block(css: str, theme: str) -> dict[str, str]:
    """Return CSS custom-property → hex value map for a [data-theme="<theme>"] block."""
    pat = re.compile(
        r':root\[data-theme=["\']' + re.escape(theme) + r'["\'][^{]*\{([^}]+)\}',
        re.DOTALL,
    )
    m = pat.search(css)
    if not m:
        return {}
    return {f"--{name}": val for name, val in _HEX.findall(m.group(1))}


def check_contrast(css: str) -> list[str]:
    failures: list[str] = []
    pairs = [
        ("--text", "--bg", "body text"),
        ("--muted", "--bg", "muted text"),
    ]
    for theme in ("light", "dark"):
        tokens = _parse_theme_block(css, theme)
        for fg_name, bg_name, label in pairs:
            fg = tokens.get(fg_name)
            bg = tokens.get(bg_name)
            if fg is None or bg is None:
                failures.append(f"[{theme}] missing token {fg_name!r} or {bg_name!r} in wyrd-tokens.css")
                continue
            ratio = contrast_ratio(fg, bg)
            if ratio < WCAG_AA:
                failures.append(
                    f"[{theme}] {label}: {fg_name}={fg} on {bg_name}={bg} "
                    f"contrast {ratio:.2f} < {WCAG_AA} (WCAG AA)"
                )
    return failures


# ─── HTML structural checks ───────────────────────────────────────────────────

_HEADING = re.compile(r'<h([1-6])\b', re.IGNORECASE)
_BUTTON_TAG = re.compile(r'<button\b([^>]*)>', re.IGNORECASE)
_ARIA_LABEL = re.compile(r'\baria-label\s*=\s*["\']([^"\']*)["\']', re.IGNORECASE)
_ARIA_EXPANDED = re.compile(r'\baria-expanded\s*=', re.IGNORECASE)
_ARIA_CONTROLS = re.compile(r'\baria-controls\s*=', re.IGNORECASE)
_CLASS = re.compile(r'\bclass\s*=\s*["\']([^"\']*)["\']', re.IGNORECASE)


def _has_main(html: str) -> bool:
    return bool(re.search(r'<main\b', html, re.IGNORECASE))


def _has_nav_with_label(html: str) -> bool:
    for m in re.finditer(r'<nav\b([^>]*)', html, re.IGNORECASE):
        if _ARIA_LABEL.search(m.group(1)):
            return True
    return False


def _heading_levels(html: str) -> list[int]:
    return [int(m.group(1)) for m in _HEADING.finditer(html)]


def _heading_order_ok(levels: list[int]) -> bool:
    for a, b in zip(levels, levels[1:]):
        if b > a + 1:
            return False
    return True


def _buttons(html: str) -> list[tuple[str, str]]:
    """Return list of (full_attrs, aria_label_or_empty) for every <button>."""
    result = []
    for m in _BUTTON_TAG.finditer(html):
        attrs = m.group(1)
        label_m = _ARIA_LABEL.search(attrs)
        result.append((attrs, label_m.group(1) if label_m else ""))
    return result


def _has_search_button_with_label(html: str) -> bool:
    for attrs, label in _buttons(html):
        if "search" in label.lower():
            return True
    return False


def _has_theme_toggle_with_label(html: str) -> bool:
    for attrs, label in _buttons(html):
        cls_m = _CLASS.search(attrs)
        classes = cls_m.group(1) if cls_m else ""
        if "theme-toggle" in classes and label:
            return True
    return False


def _has_nav_drawer_control(html: str) -> bool:
    for attrs, _label in _buttons(html):
        if _ARIA_EXPANDED.search(attrs) and _ARIA_CONTROLS.search(attrs):
            return True
    return False


def check_page(path: Path) -> list[str]:
    failures: list[str] = []
    html = path.read_text(encoding="utf-8")
    name = str(path.relative_to(BUILD_DIR))

    if not _has_main(html):
        failures.append(f"{name}: missing <main> landmark")
    if not _has_nav_with_label(html):
        failures.append(f"{name}: missing <nav> with aria-label")

    levels = _heading_levels(html)
    h1_count = levels.count(1)
    if h1_count != 1:
        failures.append(f"{name}: expected 1 <h1>, found {h1_count}")
    if not _heading_order_ok(levels):
        failures.append(f"{name}: heading levels skip a step: {levels}")

    if not _has_search_button_with_label(html):
        failures.append(f"{name}: search button missing aria-label")
    if not _has_theme_toggle_with_label(html):
        failures.append(f"{name}: theme-toggle button missing aria-label")
    if not _has_nav_drawer_control(html):
        failures.append(f"{name}: nav-drawer button missing aria-expanded/aria-controls")

    return failures


# ─── Main ─────────────────────────────────────────────────────────────────────

# The home splash (root index.html) and the Fathom teaser are full-bleed holding
# pages with no doc shell — no <main>, splash heading structure. They are exempt
# from the doc-shell a11y contract, like 404.html. index.html existence is still
# asserted separately in main() as a prerender-truncation guard.
_SKIP_FILES = frozenset({"404.html", "index.html"})
_SKIP_DIRS = frozenset({"mocks", "pagefind", "_app", "fathom"})


def _real_pages() -> list[Path]:
    pages = []
    for html in sorted(BUILD_DIR.rglob("*.html")):
        rel = html.relative_to(BUILD_DIR)
        parts = rel.parts
        # Skip root-level exclusions and excluded directory trees
        if parts[0] in _SKIP_FILES:
            continue
        if parts[0] in _SKIP_DIRS:
            continue
        pages.append(html)
    return pages


def main() -> int:
    if not BUILD_DIR.exists():
        print(f"Build dir not found: {BUILD_DIR}\nRun 'mise run docs:build' first.")
        return 1

    failures: list[str] = []

    # Fail-closed on a truncated prerender: the home route must be emitted and
    # the page count must clear the floor. A masked render failure drops pages
    # and omits index.html while exiting 0; assert both before any a11y check so
    # the gate cannot pass vacuously over a partial build.
    if not (BUILD_DIR / "index.html").exists():
        failures.append(
            "build/index.html missing — prerender truncated (home route did not emit)"
        )

    if not TOKENS_CSS.exists():
        failures.append(f"wyrd-tokens.css not found at {TOKENS_CSS}")
    else:
        failures.extend(check_contrast(TOKENS_CSS.read_text(encoding="utf-8")))

    pages = _real_pages()
    if not pages:
        failures.append("No HTML pages found in docs/build (run docs:build first)")
    elif len(pages) < MIN_REAL_PAGES:
        failures.append(
            f"Only {len(pages)} doc pages emitted, expected >= {MIN_REAL_PAGES} "
            "— prerender truncated (pages silently dropped)"
        )
    for page in pages:
        failures.extend(check_page(page))

    if failures:
        print("Docs a11y check failed:")
        print("\n".join(f"  {f}" for f in failures))
        return 1

    print(f"Docs a11y OK — {len(pages)} pages checked, contrast AA verified (light + dark)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
