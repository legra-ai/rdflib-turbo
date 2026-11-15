"""Release tags use the existing App that may tag workflow-bearing commits."""

from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]


def test_release_tag_checkout_uses_the_scoped_release_writer():
    publish = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())["jobs"]["publish"]
    steps = publish["steps"]
    writers = [step for step in steps if step.get("id") == "writer"]
    assert len(writers) == 1
    writer = writers[0]
    assert writer["uses"].startswith("actions/create-github-app-token@")
    assert writer["with"] == {
        "app-id": "${{ secrets.RELEASE_WRITER_APP_ID }}",
        "private-key": "${{ secrets.RELEASE_WRITER_APP_KEY }}",
        "owner": "${{ github.repository_owner }}",
        "repositories": "${{ github.event.repository.name }}",
        "permission-contents": "write",
        "permission-workflows": "write",
    }
    checkout = next(step for step in steps if step.get("uses", "").startswith("actions/checkout@"))
    assert checkout["with"]["token"] == "${{ steps.writer.outputs.token }}"
    assert checkout["with"]["ref"] == "${{ needs.prepare.outputs.revision }}"
    tag = next(
        step
        for step in steps
        if "tag-tested-release@" in step.get("uses", "")
        or "scripts/tag_release.py" in step.get("run", "")
    )
    assert steps.index(writer) < steps.index(checkout) < steps.index(tag)
    assert publish["env"]["GH_TOKEN"] == "${{ github.token }}"
