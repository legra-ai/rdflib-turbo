"""Publish separate measured Python/Rust coverage badges from one tested revision."""

import argparse
import html
import json
from pathlib import Path


def badge_data(label, covered, count):
    if any(type(value) is not int or value < 0 for value in (covered, count)) or covered > count:
        raise ValueError("Invalid measured coverage counts")
    tenths = covered * 1000 // count if count else None
    percentage = f"{tenths // 10}.{tenths % 10}%" if tenths is not None else "n/a"
    color = (
        "#4c1"
        if tenths is not None and tenths >= 900
        else "#a4a61d"
        if tenths is not None and tenths >= 800
        else "#dfb317"
        if tenths is not None and tenths >= 700
        else "#e05d44"
    )
    return {
        "schemaVersion": 1,
        "label": label,
        "message": percentage,
        "color": color.removeprefix("#"),
    }


def render_badge(label, covered, count):
    data = badge_data(label, covered, count)
    percentage, color = data["message"], "#" + data["color"]
    left = len(label) * 7 + 14
    width = left + 62
    title = html.escape(f"{label}: {percentage}", quote=True)
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="20" role="img" aria-label="{title}">
<title>{title}</title><rect width="{width}" height="20" rx="3" fill="#555"/>
<path d="M{left} 0h59a3 3 0 0 1 3 3v14a3 3 0 0 1-3 3h-59z" fill="{color}"/>
<g fill="#fff" text-anchor="middle" font-family="Verdana,DejaVu Sans,sans-serif" font-size="11">
<text x="{left / 2}" y="14">{html.escape(label)}</text><text x="{left + 31}" y="14">{percentage}</text>
</g></svg>
"""


def build_site(python, rust, output, project, revision):
    if python.get("meta", {}).get("branch_coverage") is not True:
        raise ValueError("Python branch coverage must be measured")
    shared = project == "owlrl-turbo"
    expected_runtime = "rdflib-turbo" if shared else project
    if (
        rust.get("project") != expected_runtime
        or rust.get("owlrl_revision" if shared else "revision") != revision
    ):
        raise ValueError("Coverage revision does not match the tested source")
    try:
        totals = python["totals"]
        lines, covered_lines, branches, covered_branches = (
            totals[key]
            for key in ("num_statements", "covered_lines", "num_branches", "covered_branches")
        )
        metrics = rust["stages"]["combined"]["owned"]
        if lines <= 0 or metrics["lines"]["count"] <= 0:
            raise ValueError("Executable source coverage is missing")
        badges = {
            "python-lines": ("Python lines", covered_lines, lines),
            "python-branches": ("Python branches", covered_branches, branches),
            "coverage": ("Python coverage", covered_lines + covered_branches, lines + branches),
        }
        for metric in ("lines", "regions", "functions"):
            badges[f"rust-{metric}"] = (
                ("Shared Rust " if shared else "Rust ") + metric,
                metrics[metric]["covered"],
                metrics[metric]["count"],
            )
    except (KeyError, TypeError) as error:
        raise ValueError("Missing measured coverage counts") from error
    payloads = {name: badge_data(*counts) for name, counts in badges.items()}
    output.mkdir(parents=True, exist_ok=True)
    for name, counts in badges.items():
        (output / f"{name}.svg").write_text(render_badge(*counts))
        (output / f"{name}.json").write_text(json.dumps(payloads[name]) + "\n")
    images = "\n".join(
        f'<p><img src="{name}.svg" alt="{name}"></p>' for name in badges if name != "coverage"
    )
    python_prefix = "owlrl-" if shared else ""
    text = f"""<!doctype html><html lang="en"><meta charset="utf-8">
<title>{html.escape(project)} measured coverage</title>
<h1>{html.escape(project)} measured coverage</h1>
<p>Source revision: <code>{html.escape(revision)}</code>. Measured {html.escape(rust["date"])}.</p>
{images}
<p>Python lines and branches are separate measurements. Rust percentages cover owned source,
merging Rust and Python integration execution. Rust branch coverage is not instrumented.
{("The Rust result covers the shared rdflib-turbo runtime, including the OWL-RL integration suite." if shared else "")}
Dependencies and vendored patches are outside the owned-source denominator.</p>
<ul><li><a href="reports/{python_prefix}python-html/index.html">Python report</a></li>
<li><a href="reports/html/index.html">Rust report</a></li>
<li><a href="reports/summary.json">Rust counts and provenance</a></li></ul>
</html>"""
    (output / "index.html").write_text(text)
    (output / ".nojekyll").touch()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--project", choices=["pyshacl-turbo", "rdflib-turbo", "owlrl-turbo"], required=True
    )
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    build_site(
        json.loads(args.python.read_text()),
        json.loads(args.rust.read_text()),
        args.output,
        args.project,
        args.revision,
    )


if __name__ == "__main__":
    main()
