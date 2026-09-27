"""pre-commit installs a hook repository with `pip install .`, so this only pulls in the wheel.

It pins the release this checkout was tagged as, read from the one version field, so a hook at
`rev: vX.Y.Z` always runs bonsai-lint X.Y.Z. See `.pre-commit-hooks.yaml`.
"""

import re
from pathlib import Path

from setuptools import setup

CARGO = Path(__file__).resolve().with_name("Cargo.toml").read_text(encoding="utf-8")
VERSION = re.search(
    r'^\[workspace\.package\]$[^\[]*?^version = "([^"]+)"$', CARGO, re.MULTILINE
).group(1)

setup(
    name="bonsai-lint-pre-commit",
    version=VERSION,
    install_requires=[f"bonsai-lint=={VERSION}"],
    packages=[],
    py_modules=[],
)
