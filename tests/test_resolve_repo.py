from pathlib import Path

from dtfls import cli


def test_uses_dtfls_repo_env_var(tmp_path, monkeypatch):
    target = tmp_path / "somewhere"
    monkeypatch.setenv("DTFLS_REPO", str(target))

    repo = cli.resolve_repo()

    assert repo == target
    assert target.is_dir()
    assert (target / ".git").is_dir()


def test_creates_default_repo_when_missing(tmp_path, monkeypatch):
    monkeypatch.delenv("DTFLS_REPO", raising=False)
    monkeypatch.setenv("HOME", str(tmp_path))

    repo = cli.resolve_repo()

    expected = Path(cli.DEFAULT_REPO_DIR).expanduser()
    assert repo == expected
    assert expected.is_dir()
    assert (expected / ".git").is_dir()


def test_does_not_reinit_existing_git_repo(tmp_path, monkeypatch):
    target = tmp_path / "existing"
    target.mkdir()
    import subprocess

    subprocess.run(["git", "init", "-q", str(target)], check=True)
    (target / "marker.txt").write_text("keep me")
    monkeypatch.setenv("DTFLS_REPO", str(target))

    repo = cli.resolve_repo()

    assert repo == target
    assert (target / "marker.txt").exists()
