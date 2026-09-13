from dtfls import cli


def test_routes_tracked_file_to_active_home_dir(repo, tmp_path, monkeypatch):
    fake_home = tmp_path / "fakehome"
    fake_home.mkdir()
    (fake_home / ".bashrc").write_text("echo hi")
    (repo / "home").mkdir()

    monkeypatch.setattr(cli, "detect_os", lambda: ("linux", "fedora"))
    monkeypatch.setenv("HOME", str(fake_home))
    config = {"track": ["~/.bashrc"]}
    result = cli.resolve_tracked_files(config)

    assert len(result) == 1
    deployed, src_name, repo_dest = result[0]
    assert deployed == fake_home / ".bashrc"
    assert src_name == "home"
    assert repo_dest == repo / "home" / ".bashrc"


def test_explicit_src_override_bypasses_routing(repo, tmp_path, monkeypatch):
    fake_home = tmp_path / "fakehome"
    fake_home.mkdir()
    (fake_home / ".bashrc").write_text("echo hi")

    monkeypatch.setattr(cli, "detect_os", lambda: ("linux", "fedora"))
    monkeypatch.setenv("HOME", str(fake_home))
    config = {"track": [{"path": "~/.bashrc", "src": "home.linux"}]}
    result = cli.resolve_tracked_files(config)

    assert len(result) == 1
    _, src_name, repo_dest = result[0]
    assert src_name == "home.linux"
    assert repo_dest == repo / "home.linux" / ".bashrc"


def test_os_filtered_entries_are_skipped(repo, tmp_path, monkeypatch):
    fake_home = tmp_path / "fakehome"
    fake_home.mkdir()
    (fake_home / ".bash_profile").write_text("echo hi")

    monkeypatch.setattr(cli, "detect_os", lambda: ("darwin", None))
    monkeypatch.setenv("HOME", str(fake_home))
    config = {"track": [{"path": "~/.bash_profile", "os": "linux"}]}
    result = cli.resolve_tracked_files(config)

    assert result == []
