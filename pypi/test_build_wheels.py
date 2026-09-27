"""Tests for build_wheels.py, against dist-shaped archives holding synthetic binaries.

    python3 -m unittest discover pypi
"""

from __future__ import annotations

import base64
import csv
import email.parser
import hashlib
import io
import json
import stat
import struct
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

import build_wheels
from build_wheels import BuildError

TESTDATA = Path(__file__).resolve().parent / "testdata"
TAG = "v0.4.1"

README = (
    "# bonsai-lint\n"
    "![cover](docs/images/cover.jpg)\n"
    "See [the rules](docs/scoring-rules.md), [the site](https://bonsai.kauneckas.dev), "
    "[below](#use-it) and [the extension](./editors/vscode).\n"
    '<img src="editors/vscode/images/diagnostic.png" alt="a diagnostic">\n'
    '| `<script src="./logic.ts">` | `[kept](as/is)` |\n'
    "```bash\n"
    "[kept](as/is)\n"
    "```\n"
)
LICENSE = b"MIT License\n\nCopyright (c) 2026\n"


def elf(arch: str, interpreter: bool) -> bytes:
    header = bytearray(64)
    header[0:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<H", header, 0x12, build_wheels.ELF_MACHINES[arch])
    struct.pack_into("<Q", header, 0x20, 64)
    struct.pack_into("<HH", header, 0x36, 56, 1)
    program = bytearray(56)
    struct.pack_into("<I", program, 0, 3 if interpreter else 1)
    return bytes(header + program) + b"elf payload"


def macho(arch: str, minos: tuple[int, int], legacy: bool = False) -> bytes:
    version = minos[0] << 16 | minos[1] << 8
    if legacy:
        command = struct.pack("<IIII", 0x24, 16, version, version)
    else:
        command = struct.pack("<IIIIII", 0x32, 24, 1, version, version, 0)
    cpu = build_wheels.MACHO_CPUS[arch]
    header = struct.pack("<IIIIIIII", 0xFEEDFACF, cpu, 0, 2, 1, len(command), 0, 0)
    return header + command + b"macho payload"


def pe(machine: int = 0x8664) -> bytes:
    data = bytearray(0x80)
    data[0:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x40)
    data[0x40:0x44] = b"PE\0\0"
    struct.pack_into("<H", data, 0x44, machine)
    return bytes(data) + b"pe payload"


BINARIES = {
    "aarch64-apple-darwin": macho("arm64", (11, 0)),
    "x86_64-apple-darwin": macho("x86_64", (10, 12), legacy=True),
    "aarch64-unknown-linux-gnu": elf("aarch64", interpreter=True),
    "x86_64-unknown-linux-gnu": elf("x86_64", interpreter=True),
    "aarch64-unknown-linux-musl": elf("aarch64", interpreter=False),
    "x86_64-unknown-linux-musl": elf("x86_64", interpreter=False),
    "x86_64-pc-windows-msvc": pe(),
}

WHEELS = {
    "aarch64-apple-darwin": "bonsai_lint-0.4.1-py3-none-macosx_11_0_arm64.whl",
    "x86_64-apple-darwin": "bonsai_lint-0.4.1-py3-none-macosx_10_12_x86_64.whl",
    "aarch64-unknown-linux-musl": "bonsai_lint-0.4.1-py3-none-manylinux2014_aarch64"
    ".manylinux_2_17_aarch64.musllinux_1_1_aarch64.whl",
    "x86_64-unknown-linux-musl": "bonsai_lint-0.4.1-py3-none-manylinux2014_x86_64"
    ".manylinux_2_17_x86_64.musllinux_1_1_x86_64.whl",
    "x86_64-pc-windows-msvc": "bonsai_lint-0.4.1-py3-none-win_amd64.whl",
}


def tarball(path: Path, triple: str, binary: bytes) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for name, data, mode in (
            ("bonsai-lint", binary, 0o755),
            ("README.md", README.encode(), 0o644),
            ("LICENSE", LICENSE, 0o644),
            ("CHANGELOG.md", b"# Changelog\n", 0o644),
        ):
            member = tarfile.TarInfo(f"bonsai-lint-{triple}/{name}")
            member.size = len(data)
            member.mode = mode
            archive.addfile(member, io.BytesIO(data))


def flat_zip(path: Path, binary: bytes) -> None:
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("bonsai-lint.exe", binary)
        archive.writestr("README.md", README.replace("\n", "\r\n"))
        archive.writestr("LICENSE", LICENSE.replace(b"\n", b"\r\n"))
        archive.writestr("CHANGELOG.md", "# Changelog\r\n")


class Release:
    """A GitHub Release's archives and manifest, as dist lays them out, in a temp directory."""

    def __init__(self, binaries: dict[str, bytes] | None = None) -> None:
        self._dir = tempfile.TemporaryDirectory()
        self.root = Path(self._dir.name)
        self.archives = self.root / "archives"
        self.archives.mkdir()
        self.manifest = json.loads((TESTDATA / "dist-manifest-0.4.1.json").read_text())
        for name, artifact in self.manifest["artifacts"].items():
            if artifact.get("kind") != "executable-zip":
                continue
            (triple,) = artifact["target_triples"]
            binary = (binaries or BINARIES)[triple]
            path = self.archives / name
            if name.endswith(".zip"):
                flat_zip(path, binary)
            else:
                tarball(path, triple, binary)
            artifact["checksums"]["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        self.save()

    def save(self) -> None:
        (self.root / "dist-manifest.json").write_text(json.dumps(self.manifest))

    def artifact(self, triple: str) -> dict:
        return next(
            artifact
            for artifact in self.manifest["artifacts"].values()
            if artifact.get("target_triples") == [triple]
        )

    def build(self, out: str = "wheels", tag: str = TAG, suffix: str = "") -> list[Path]:
        built = build_wheels.build_release(
            self.root / "dist-manifest.json", self.archives, tag, self.root / out, suffix
        )
        return [wheel for wheel, _ in built]

    def close(self) -> None:
        self._dir.cleanup()


def wheel_for(wheels: list[Path], triple: str) -> Path:
    return next(wheel for wheel in wheels if wheel.name == WHEELS[triple])


def read_metadata(wheel: Path) -> email.message.Message:
    with zipfile.ZipFile(wheel) as archive:
        name = next(n for n in archive.namelist() if n.endswith(".dist-info/METADATA"))
        return email.parser.Parser().parsestr(archive.read(name).decode())


class ReleaseWheels(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.release = Release()
        cls.wheels = cls.release.build()

    @classmethod
    def tearDownClass(cls) -> None:
        cls.release.close()

    def test_five_wheels_carry_the_published_filenames(self) -> None:
        self.assertEqual(sorted(w.name for w in self.wheels), sorted(WHEELS.values()))

    def test_the_gnu_builds_are_not_packaged(self) -> None:
        self.assertNotIn("x86_64-unknown-linux-gnu", build_wheels.RELEASE_SLOTS)
        self.assertNotIn("aarch64-unknown-linux-gnu", build_wheels.RELEASE_SLOTS)
        platforms = [
            platform
            for wheel in self.wheels
            for platform in wheel.name.removesuffix(".whl").split("-py3-none-")[1].split(".")
        ]
        self.assertFalse([p for p in platforms if p.startswith("linux_")])
        self.assertFalse([p for p in platforms if p.startswith("manylinux_2_3")])

    def test_each_wheel_carries_its_archives_binary_byte_for_byte(self) -> None:
        for triple in WHEELS:
            slot = build_wheels.RELEASE_SLOTS[triple]
            with zipfile.ZipFile(wheel_for(self.wheels, triple)) as archive:
                binary = archive.read(f"bonsai_lint-0.4.1.data/scripts/{slot.executable}")
            self.assertEqual(binary, BINARIES[triple], triple)

    def test_the_binary_is_an_executable_regular_file(self) -> None:
        for wheel in self.wheels:
            with zipfile.ZipFile(wheel) as archive:
                (script,) = [i for i in archive.infolist() if "/scripts/" in i.filename]
                self.assertEqual(script.external_attr >> 16, stat.S_IFREG | 0o755, wheel.name)

    def test_record_lists_every_file_with_its_hash_and_size(self) -> None:
        for wheel in self.wheels:
            with zipfile.ZipFile(wheel) as archive:
                names = archive.namelist()
                record = next(n for n in names if n.endswith("RECORD"))
                rows = list(csv.reader(io.StringIO(archive.read(record).decode())))
                self.assertEqual(sorted(r[0] for r in rows), sorted(names))
                for name, digest, size in rows:
                    if name == record:
                        self.assertEqual((digest, size), ("", ""))
                        continue
                    data = archive.read(name)
                    expected = base64.urlsafe_b64encode(hashlib.sha256(data).digest())
                    self.assertEqual(digest, "sha256=" + expected.rstrip(b"=").decode())
                    self.assertEqual(size, str(len(data)))

    def test_the_wheel_tags_are_the_filenames_platforms(self) -> None:
        for wheel in self.wheels:
            platforms = wheel.name.removesuffix(".whl").split("-py3-none-")[1].split(".")
            with zipfile.ZipFile(wheel) as archive:
                meta = archive.read("bonsai_lint-0.4.1.dist-info/WHEEL").decode()
            tags = [line.split(": ")[1] for line in meta.splitlines() if line.startswith("Tag:")]
            self.assertEqual(tags, [f"py3-none-{p}" for p in platforms])
            self.assertIn("Root-Is-Purelib: false", meta)

    def test_the_metadata_comes_from_the_crate(self) -> None:
        project = build_wheels.read_project()
        meta = read_metadata(self.wheels[0])
        self.assertEqual(meta["Metadata-Version"], "2.4")
        self.assertEqual(meta["Name"], "bonsai-lint")
        self.assertEqual(meta["Version"], "0.4.1")
        self.assertEqual(meta["Summary"], project.summary)
        self.assertEqual(meta["Keywords"], ",".join(project.keywords))
        self.assertEqual(meta["License-Expression"], "MIT")
        self.assertEqual(meta["License-File"], "LICENSE")
        self.assertEqual(meta["Description-Content-Type"], "text/markdown")
        self.assertIsNone(meta["Requires-Python"])
        self.assertIn(
            "Changelog, https://github.com/ryckakas/bonsai-lint/blob/v0.4.1/CHANGELOG.md",
            meta.get_all("Project-URL"),
        )

    def test_every_wheel_ships_the_same_lf_license(self) -> None:
        for wheel in self.wheels:
            with zipfile.ZipFile(wheel) as archive:
                license_text = archive.read("bonsai_lint-0.4.1.dist-info/licenses/LICENSE")
            self.assertEqual(license_text, LICENSE, wheel.name)

    def test_readme_links_become_absolute_at_the_tag(self) -> None:
        description = read_metadata(self.wheels[0]).get_payload()
        blob = "https://github.com/ryckakas/bonsai-lint/blob/v0.4.1/"
        raw = "https://raw.githubusercontent.com/ryckakas/bonsai-lint/v0.4.1/"
        self.assertIn(f"![cover]({raw}docs/images/cover.jpg)", description)
        self.assertIn(f"[the rules]({blob}docs/scoring-rules.md)", description)
        self.assertIn(f"[the extension]({blob}editors/vscode)", description)
        self.assertIn(f'<img src="{raw}editors/vscode/images/diagnostic.png"', description)
        self.assertIn("[the site](https://bonsai.kauneckas.dev)", description)
        self.assertIn("[below](#use-it)", description)
        self.assertIn('`<script src="./logic.ts">`', description)
        self.assertEqual(description.count("[kept](as/is)"), 2)

    def test_two_builds_are_byte_identical(self) -> None:
        again = self.release.build(out="again")
        for first, second in zip(sorted(self.wheels), sorted(again)):
            self.assertEqual(first.read_bytes(), second.read_bytes(), first.name)


class ReleaseRefusals(unittest.TestCase):
    def setUp(self) -> None:
        self.release = Release()

    def tearDown(self) -> None:
        self.release.close()

    def assert_refused(self, message: str, **build: str) -> None:
        with self.assertRaises(BuildError) as raised:
            self.release.build(**build)
        self.assertIn(message, str(raised.exception))

    def test_a_checksum_mismatch(self) -> None:
        self.release.artifact("x86_64-pc-windows-msvc")["checksums"]["sha256"] = "0" * 64
        self.release.save()
        self.assert_refused("sha256 differs")

    def test_a_missing_target(self) -> None:
        self.release.artifact("aarch64-unknown-linux-musl")["kind"] = "source-tarball"
        self.release.save()
        self.assert_refused("no archive for aarch64-unknown-linux-musl")

    def test_an_xz_archive(self) -> None:
        artifacts = self.release.manifest["artifacts"]
        name = "bonsai-lint-aarch64-apple-darwin.tar.gz"
        artifacts[name.replace(".gz", ".xz")] = artifacts.pop(name)
        self.release.save()
        self.assert_refused("expected a .tar.gz archive")

    def test_a_tag_the_manifest_does_not_announce(self) -> None:
        self.assert_refused("announces v0.4.1, not v0.4.2", tag="v0.4.2")

    def test_a_suffix_that_is_not_dev(self) -> None:
        self.assert_refused("must be .devN", suffix=".post1")


class BinaryRefusals(unittest.TestCase):
    def assert_refused(self, triple: str, binary: bytes, message: str) -> None:
        release = Release(dict(BINARIES, **{triple: binary}))
        try:
            with self.assertRaises(BuildError) as raised:
                release.build()
            self.assertIn(message, str(raised.exception))
        finally:
            release.close()

    def test_a_dynamic_binary_in_a_musl_slot(self) -> None:
        self.assert_refused(
            "x86_64-unknown-linux-musl", elf("x86_64", interpreter=True), "dynamic loader"
        )

    def test_a_binary_for_the_wrong_architecture(self) -> None:
        self.assert_refused(
            "aarch64-unknown-linux-musl", elf("x86_64", interpreter=False), "is not aarch64"
        )

    def test_a_minimum_macos_newer_than_the_tag(self) -> None:
        self.assert_refused("aarch64-apple-darwin", macho("arm64", (12, 0)), "needs macOS 12.0")

    def test_a_windows_binary_for_arm(self) -> None:
        self.assert_refused("x86_64-pc-windows-msvc", pe(0xAA64), "is not x86_64")


class Versions(unittest.TestCase):
    def test_a_dev_suffix_reaches_the_filenames_and_metadata(self) -> None:
        release = Release()
        try:
            wheels = release.build(suffix=".dev7")
            self.assertTrue(all(w.name.startswith("bonsai_lint-0.4.1.dev7-") for w in wheels))
            self.assertEqual(read_metadata(wheels[0])["Version"], "0.4.1.dev7")
        finally:
            release.close()

    def test_the_real_manifest_names_every_release_target(self) -> None:
        manifest = json.loads((TESTDATA / "dist-manifest-0.4.1.json").read_text())
        version, artifacts = build_wheels.release_artifacts(manifest, TAG)
        self.assertEqual(version, "0.4.1")
        self.assertLessEqual(set(build_wheels.RELEASE_SLOTS), set(artifacts))


class LocalBinary(unittest.TestCase):
    def test_a_local_gnu_build_gets_a_linux_tag_that_pypi_refuses(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "bonsai-lint"
            binary.write_bytes(elf("x86_64", interpreter=True))
            wheel = build_wheels.build_binary(
                binary, "x86_64-unknown-linux-gnu", Path(directory) / "out"
            )
            version = build_wheels.read_project().version
            self.assertEqual(wheel.name, f"bonsai_lint-{version}-py3-none-linux_x86_64.whl")

    def test_an_unknown_target_is_refused(self) -> None:
        with self.assertRaises(BuildError):
            build_wheels.build_binary(Path("unused"), "riscv64gc-unknown-linux-gnu", Path("out"))


if __name__ == "__main__":
    unittest.main()
