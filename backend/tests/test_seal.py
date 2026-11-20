import json
import sys
import types

import pytest


class StandInKeyring(types.ModuleType):
    def __init__(self):
        super().__init__("keyring")
        self.store = {}

    def get_password(self, service, account):
        return self.store.get((service, account))

    def set_password(self, service, account, value):
        self.store[(service, account)] = value


@pytest.fixture()
def folder(tmp_path, monkeypatch):
    from app.core import seal
    from app.services import agents, claude_code

    monkeypatch.setattr(agents, "SETTINGS", tmp_path / "agents.json")
    monkeypatch.setattr(claude_code, "PREFS", tmp_path / "claude.json")
    monkeypatch.setattr(claude_code, "_cache", {"at": 0.0, "value": None})
    seal.reset()
    yield tmp_path
    seal.reset()


@pytest.fixture()
def keychain(monkeypatch, folder):
    from app.core import seal

    fake = StandInKeyring()
    monkeypatch.setitem(sys.modules, "keyring", fake)
    seal.reset()
    return fake


@pytest.fixture()
def no_keychain(monkeypatch, folder):
    from app.core import seal

    monkeypatch.setitem(sys.modules, "keyring", None)
    seal.reset()


def edit(path, change):
    data = json.loads(path.read_text(encoding="utf-8"))
    change(data)
    path.write_text(json.dumps(data), encoding="utf-8")


def test_a_choice_made_in_the_window_is_sealed_and_stands(folder, keychain):
    from app.services import agents, claude_code

    agents.set_policy("Codex", "allow")
    claude_code.save_prefs({"claudeBin": "/bin/sh"})
    c = agents.choice()
    assert c["seal"] == "ok" and not c["sealBroken"]
    assert agents.policy("Codex") == "allow"
    assert claude_code.prefs()["claudeBin"] == "/bin/sh"
    assert json.loads((folder / "agents.json").read_text(encoding="utf-8"))["seal"]


def test_a_hand_edited_policy_is_caught(folder, keychain):
    from app.services import agents

    agents.set_policy("Codex", "ask")
    edit(folder / "agents.json", lambda d: d["policies"].__setitem__("Codex", "allow"))
    assert agents.policy("Codex") == "ask"
    assert agents.snapshot()["sealBroken"] is True


def test_a_hand_edited_program_is_caught(folder, keychain):
    from app.services import agents, claude_code

    claude_code.save_prefs({"claudeFolder": "/tmp"})
    edit(folder / "claude.json", lambda d: d.__setitem__("claudeBin", "/tmp/other-program"))
    assert claude_code.prefs()["claudeBin"] == ""
    assert agents.choice()["sealBroken"] is True


@pytest.mark.parametrize("kind,key", [("codex", "codexBin"), ("cursor", "cursorBin"), ("antigravity", "antigravityBin")])
def test_the_other_programs_paths_are_sealed_and_given_to_the_finder_first(folder, keychain, kind, key):
    from app.services import agent_lookup, agents, claude_code

    claude_code.save_prefs({key: "/bin/sh"})
    assert agents.choice()["seal"] == "ok"
    assert claude_code.setting(kind) == "/bin/sh"
    found = agent_lookup.find(kind, setting=claude_code.setting(kind), env={}, home=str(folder))
    assert (found.path, found.how) == ("/bin/sh", "setting")
    assert claude_code.prefs()[key] == "/bin/sh"
    edit(folder / "claude.json", lambda d: d.__setitem__(key, "/tmp/other-program"))
    assert claude_code.setting(kind) == ""
    assert agents.choice()["sealBroken"] is True


def test_a_choice_sealed_by_0_91_0_stays_whole_and_keeps_the_claude_code_path(folder, keychain):
    import hashlib
    import hmac

    from app.core import seal
    from app.services import agents, claude_code

    key = seal._load(True)
    old = {"enabled": True, "policies": {"Codex": "allow"}, "claudeBin": "/bin/sh"}
    body = json.dumps(old, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    mark = hmac.new(key, body.encode("utf-8"), hashlib.sha256).hexdigest()
    (folder / "agents.json").write_text(json.dumps({"enabled": True, "policies": {"Codex": "allow"}, "seal": mark}, indent=1), encoding="utf-8")
    (folder / "claude.json").write_text(json.dumps({"claudeBin": "/bin/sh", "claudeFolder": "/tmp"}, indent=1), encoding="utf-8")
    c = agents.choice()
    assert c["seal"] == "ok" and c["sealBroken"] is False
    assert agents.policy("Codex") == "allow" and claude_code.setting("claude") == "/bin/sh"
    assert [c[k] for k in ("codexBin", "cursorBin", "antigravityBin")] == ["", "", ""]
    claude_code.save_prefs({"claudeFirstMessage": "hello"})
    claude_code.save_prefs({"cursorBin": "/bin/sh"})
    c = agents.choice()
    assert c["seal"] == "ok" and agents.policy("Codex") == "allow" and c["claudeBin"] == "/bin/sh" and c["cursorBin"] == "/bin/sh"


def test_tightening_by_hand_is_not_a_break(folder, keychain):
    from app.services import agents

    agents.set_policy("Codex", "ask")
    edit(folder / "agents.json", lambda d: d["policies"].__setitem__("Codex", "deny"))
    assert agents.policy("Codex") == "deny"
    assert agents.choice()["sealBroken"] is False


def test_a_window_write_confirms_the_choice_again(folder, keychain):
    from app.services import agents, claude_code

    agents.set_policy("Codex", "ask")
    claude_code.save_prefs({"claudeBin": "/bin/sh"})
    edit(folder / "claude.json", lambda d: d.__setitem__("claudeBin", "/tmp/other-program"))
    assert agents.choice()["sealBroken"] is True
    agents.set_policy("Codex", "ask")
    assert agents.choice()["sealBroken"] is False
    assert claude_code.prefs()["claudeBin"] == ""


def test_an_older_loosened_choice_without_a_seal_is_held_until_confirmed(folder, keychain):
    from app.services import agents

    (folder / "agents.json").write_text(json.dumps({"policies": {"Codex": "allow"}}), encoding="utf-8")
    assert agents.policy("Codex") == "ask"
    assert agents.choice()["sealBroken"] is True


def test_with_no_keychain_the_stored_values_stand_and_the_seal_is_unavailable(folder, no_keychain):
    from app.services import agents, claude_code

    agents.set_policy("Codex", "allow")
    claude_code.save_prefs({"claudeBin": "/bin/sh"})
    c = agents.choice()
    assert c["seal"] == "unavailable" and c["sealBroken"] is False
    assert agents.policy("Codex") == "allow"
    assert claude_code.prefs()["claudeBin"] == "/bin/sh"
    assert "seal" not in json.loads((folder / "agents.json").read_text(encoding="utf-8"))
    assert agents.snapshot()["seal"] == "unavailable"


def test_a_keychain_that_raises_is_treated_as_unavailable(folder, monkeypatch):
    from app.core import seal
    from app.services import agents

    class Locked(StandInKeyring):
        def get_password(self, service, account):
            raise RuntimeError("keychain is locked")

    monkeypatch.setitem(sys.modules, "keyring", Locked())
    seal.reset()
    agents.set_policy("Codex", "allow")
    assert agents.choice()["seal"] == "unavailable"
    assert agents.policy("Codex") == "allow"
