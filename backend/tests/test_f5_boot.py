from conftest import APP_ID, HEALTH_PATH, VERSION_KEY, VERSION_PATH, get_json


def test_engine_boots_on_a_free_port_with_an_empty_data_folder(engine):
    status, _ = get_json(engine["base"] + HEALTH_PATH)
    assert status == 200
    assert engine["proc"].poll() is None


def test_engine_reports_the_repository_version(engine, repo_version):
    status, body = get_json(engine["base"] + VERSION_PATH)
    assert status == 200
    assert str(body[VERSION_KEY]).lstrip("v") == repo_version.lstrip("v")


def test_engine_uses_the_data_folder_it_was_given(engine):
    assert any(engine["data"].iterdir()), "the engine set up its folders in the data folder"
    home = engine["data"].parent / "home"
    assert not [p.name for p in home.iterdir() if p.name.lower().startswith(APP_ID.lower())]
