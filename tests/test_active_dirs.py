from dtfls import cli


def test_falls_back_to_repo_root_when_no_home_dirs(repo, monkeypatch):
    monkeypatch.setattr(cli, "detect_os", lambda: ("linux", "fedora"))

    dirs = cli.active_dirs({})

    assert dirs == [(".", cli.Path.home())]


def test_picks_most_specific_active_dir(repo, monkeypatch):
    monkeypatch.setattr(cli, "detect_os", lambda: ("linux", "fedora"))
    (repo / "home").mkdir()
    (repo / "home.linux").mkdir()
    (repo / "home.fedora").mkdir()

    dirs = cli.active_dirs({})

    assert [name for name, _ in dirs] == ["home", "home.linux", "home.fedora"]


def test_explicit_mappings_filter_by_os_and_distro(repo, monkeypatch):
    monkeypatch.setattr(cli, "detect_os", lambda: ("darwin", None))
    (repo / "home").mkdir()
    (repo / "home.fedora-only").mkdir()
    config = {
        "mappings": [
            {"src": "home", "dest": "~"},
            {"src": "home.fedora-only", "dest": "~", "os": "linux", "distro": "fedora"},
        ]
    }

    dirs = cli.active_dirs(config)

    assert [name for name, _ in dirs] == ["home"]
