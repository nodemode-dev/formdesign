import re

VERSION = re.compile(r"^\d+\.\d+\.\d$")


def test_every_version_site_was_found(repo_versions):
    missing = [site for site, value in repo_versions.items() if value is None]
    assert not missing, f"no version found in {missing}"


def test_every_version_site_agrees(repo_versions):
    assert len(set(repo_versions.values())) == 1, repo_versions


def test_version_has_three_parts_and_a_one_digit_patch(repo_version):
    assert VERSION.match(repo_version.lstrip("v")), repo_version
