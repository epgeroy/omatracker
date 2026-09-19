"""Validate local Markdown links and heading fragments in distributable docs."""
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit


def prose(path):
    # Shell examples contain [] and # that are not Markdown links/headings.
    return re.sub(r"^```[^\n]*\n.*?^```\s*$", "", path.read_text(), flags=re.M | re.S)


def anchors(path):
    result = set()
    counts = {}
    for heading in re.findall(r"^#{1,6}\s+(.+?)\s*#*\s*$", prose(path), re.M):
        heading = re.sub(r"\[([^]]+)\]\([^)]+\)", r"\1", heading)
        slug = re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")
        count = counts.get(slug, 0)
        result.add(f"{slug}-{count}" if count else slug)
        counts[slug] = count + 1
    result.update(re.findall(r'<a\s+(?:id|name)="([^"]+)"', prose(path)))
    return result


def check_links(documents, boundary):
    boundary = boundary.resolve()
    checked = 0
    for document in documents:
        for href in re.findall(r"\[[^]\n]+\]\(([^)\s]+)\)", prose(document)):
            link = urlsplit(href)
            if link.scheme or link.netloc:
                continue
            target = (document.parent / unquote(link.path)).resolve() if link.path else document.resolve()
            assert target.is_relative_to(boundary), f"{document}: link escapes bundle: {href}"
            assert target.is_file(), f"{document}: missing link target: {href}"
            if link.fragment:
                assert unquote(link.fragment) in anchors(target), f"{document}: missing heading: {href}"
            checked += 1
    assert checked, "No documentation links checked"
    return checked
