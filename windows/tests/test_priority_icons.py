from pathlib import Path

import pytest

from taskline import priority_icons

REPO_ICONS = Path(__file__).resolve().parents[2] / "assets" / "icons"


def test_parse_lucide_paths_and_lines():
    high = priority_icons.parse_svg((priority_icons.ICON_DIR / "priority-high.svg").read_text())
    assert high == [[(17, 11), (12, 6), (7, 11)], [(17, 18), (12, 13), (7, 18)]]
    medium = priority_icons.parse_svg((priority_icons.ICON_DIR / "priority-medium.svg").read_text())
    assert medium == [[(5, 9), (19, 9)], [(5, 15), (19, 15)]]
    low = priority_icons.parse_svg((priority_icons.ICON_DIR / "priority-low.svg").read_text())
    assert low == [[(6, 9), (12, 15), (18, 9)]]


def test_render_draws_in_the_given_color():
    image = priority_icons.render("H", "#f7768e", 18)
    assert image.size == (18, 18)
    data = image.tobytes()
    pixels = [tuple(data[i:i + 4]) for i in range(0, len(data), 4)]
    opaque = [px for px in pixels if px[3] > 200]
    assert opaque, "icon is empty"
    assert all(abs(r - 0xF7) < 8 and abs(g - 0x76) < 8 and abs(b - 0x8E) < 8 for r, g, b, _ in opaque)
    assert priority_icons.render("", "#fff") is None
    assert priority_icons.render(None, "#fff") is None


def test_icons_differ_by_priority():
    images = [priority_icons.render(p, "#000000", 24).tobytes() for p in "HML"]
    assert len(set(images)) == 3


@pytest.mark.skipif(not REPO_ICONS.exists(), reason="repository assets not available")
def test_bundled_icons_match_the_repository_copy():
    for name in [*priority_icons.FILES.values(), "LICENSE-lucide.txt"]:
        assert (priority_icons.ICON_DIR / name).read_bytes() == (REPO_ICONS / name).read_bytes(), name
