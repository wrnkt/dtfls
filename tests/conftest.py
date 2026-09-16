import subprocess

import pytest

from dtfls import cli


@pytest.fixture
def repo(tmp_path, monkeypatch):
    """A tmp dir set as the module's REPO, initialized as a git repo."""
    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    subprocess.run(["git", "-C", str(tmp_path), "config", "user.email", "t@t.com"])
    subprocess.run(["git", "-C", str(tmp_path), "config", "user.name", "t"])
    monkeypatch.setattr(cli, "REPO", tmp_path)
    return tmp_path
