import json

from dtfls import cli


def test_load_config_defaults_when_no_file(repo):
    cfg = cli.load_config()

    assert cfg["ignore"] == cli.DEFAULT_IGNORE
    assert cfg["hooks"] == {"pre_sync": [], "post_sync": []}
    assert "track" not in cfg
    assert "mappings" not in cfg


def test_load_config_merges_ignore_patterns(repo):
    (repo / cli.CONFIG_NAME).write_text(json.dumps({"ignore": ["secrets/", "*.swp"]}))

    cfg = cli.load_config()

    assert "secrets/" in cfg["ignore"]
    # *.swp is already a default pattern and should not be duplicated.
    assert cfg["ignore"].count("*.swp") == 1


def test_load_config_passes_through_track_and_mappings(repo):
    user_cfg = {
        "track": ["~/.bashrc"],
        "mappings": [{"src": "home", "dest": "~"}],
        "hooks": {"pre_sync": ["echo hi"], "post_sync": []},
    }
    (repo / cli.CONFIG_NAME).write_text(json.dumps(user_cfg))

    cfg = cli.load_config()

    assert cfg["track"] == ["~/.bashrc"]
    assert cfg["mappings"] == [{"src": "home", "dest": "~"}]
    assert cfg["hooks"]["pre_sync"] == ["echo hi"]
