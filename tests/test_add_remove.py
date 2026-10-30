import argparse
import json

from dtfls import cli


def _setup(repo, tmp_path, monkeypatch, distro="fedora"):
    home = tmp_path / "fakehome"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setattr(cli, "detect_os", lambda: ("linux", distro))
    cli.ensure_config()
    return home


def test_route_picks_most_specific_dir(tmp_path):
    home = tmp_path / "h"
    dirs = [("home", home), ("home.linux", home), ("home.fedora", home)]

    assert cli.route(home / ".bashrc", dirs) == ("home.fedora", home)
    assert cli.route(tmp_path / "elsewhere", dirs) is None


def test_add_routes_to_same_dir_as_adopt(repo, tmp_path, monkeypatch):
    home = _setup(repo, tmp_path, monkeypatch)
    (repo / "home").mkdir()
    (repo / "home.linux").mkdir()
    (home / ".bashrc").write_text("echo hi")

    cli.cmd_add(argparse.Namespace(files=[str(home / ".bashrc")], message=None))

    assert (repo / "home.linux" / ".bashrc").read_text() == "echo hi"
    assert not (repo / "home" / ".bashrc").exists()
    _, src_name, _ = cli.resolve_tracked_files(cli.load_config())[0]
    assert src_name == "home.linux"


def test_add_with_object_track_entries_does_not_crash(repo, tmp_path, monkeypatch):
    home = _setup(repo, tmp_path, monkeypatch)
    (repo / cli.CONFIG_NAME).write_text(
        json.dumps({"track": [{"path": "~/.zprofile", "os": "darwin"}, "~/.bashrc"]})
    )
    (home / ".bashrc").write_text("echo hi")
    (home / ".vimrc").write_text("set nu")

    cli.cmd_add(
        argparse.Namespace(
            files=[str(home / ".bashrc"), str(home / ".vimrc")], message=None
        )
    )

    track = cli.load_user_config()["track"]
    assert track == [{"path": "~/.zprofile", "os": "darwin"}, "~/.bashrc", "~/.vimrc"]


def test_remove_reports_and_deletes_the_matching_bucket(
    repo, tmp_path, monkeypatch, capsys
):
    home = _setup(repo, tmp_path, monkeypatch)
    (repo / "home").mkdir()
    (repo / "home.fedora").mkdir()
    (repo / "home" / ".bashrc").write_text("echo hi")
    cli._git("add", "-A")
    cli._git("commit", "-m", "init")

    cli.cmd_remove(argparse.Namespace(files=[str(home / ".bashrc")], message=None))

    assert not (repo / "home" / ".bashrc").exists()
    out = capsys.readouterr().out
    assert "Removed from repo: home/.bashrc" in out


def test_remove_accepts_repo_path(repo, tmp_path, monkeypatch):
    _setup(repo, tmp_path, monkeypatch)
    (repo / "home").mkdir()
    (repo / "home" / ".vimrc").write_text("set nu")
    cli._git("add", "-A")
    cli._git("commit", "-m", "init")

    cli.cmd_remove(
        argparse.Namespace(files=[str(repo / "home" / ".vimrc")], message=None)
    )

    assert not (repo / "home" / ".vimrc").exists()
