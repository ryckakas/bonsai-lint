#!/usr/bin/env python3
"""Repackage dist's release archives as PyPI wheels, so pip installs the binary the Release ships.

`release` builds the five published wheels from a GitHub Release's archives and manifest.
`binary` wraps one local build for the platform it was built on, which is how CI tests the
pre-commit hook before anything is released. Stdlib only; Python 3.11+ for tomllib.

    python3 pypi/build_wheels.py release --manifest dist-manifest.json --archives DIR \
        --tag v0.4.2 --out wheels
    python3 pypi/build_wheels.py binary --binary target/debug/bonsai-lint \
        --target aarch64-apple-darwin --out wheels
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import io
import json
import re
import stat
import struct
import sys
import tarfile
import tomllib
import zipfile
from dataclasses import dataclass
from pathlib import Path

NAME = "bonsai-lint"
STEM = "bonsai_lint"
ROOT = Path(__file__).resolve().parent.parent
HOMEPAGE = "https://bonsai.kauneckas.dev"
ZIP_EPOCH = (1980, 1, 1, 0, 0, 0)
DOCS_TARGET = "x86_64-unknown-linux-musl"


class BuildError(Exception):
    pass


@dataclass(frozen=True)
class Slot:
    tags: tuple[str, ...]
    kind: str
    arch: str
    minos: tuple[int, int] | None = None

    @property
    def executable(self) -> str:
        return f"{NAME}.exe" if self.kind == "pe" else NAME

    @property
    def platform(self) -> str:
        return ".".join(sorted(self.tags))


# The gnu builds are left out on purpose: the static musl binary runs on every glibc from 2.17
# and on Alpine, so one Linux wheel per architecture serves both.
RELEASE_SLOTS = {
    "aarch64-apple-darwin": Slot(("macosx_11_0_arm64",), "macho", "arm64", (11, 0)),
    "x86_64-apple-darwin": Slot(("macosx_10_12_x86_64",), "macho", "x86_64", (10, 12)),
    "aarch64-unknown-linux-musl": Slot(
        ("manylinux_2_17_aarch64", "manylinux2014_aarch64", "musllinux_1_1_aarch64"),
        "elf-static",
        "aarch64",
    ),
    "x86_64-unknown-linux-musl": Slot(
        ("manylinux_2_17_x86_64", "manylinux2014_x86_64", "musllinux_1_1_x86_64"),
        "elf-static",
        "x86_64",
    ),
    "x86_64-pc-windows-msvc": Slot(("win_amd64",), "pe", "x86_64"),
}

# pip installs a `linux_*` wheel but PyPI refuses one, so a local gnu build can never be uploaded.
LOCAL_SLOTS = {
    **RELEASE_SLOTS,
    "aarch64-unknown-linux-gnu": Slot(("linux_aarch64",), "elf", "aarch64"),
    "x86_64-unknown-linux-gnu": Slot(("linux_x86_64",), "elf", "x86_64"),
}

ELF_MACHINES = {"x86_64": 62, "aarch64": 183}
MACHO_CPUS = {"x86_64": 0x01000007, "arm64": 0x0100000C}
PE_MACHINES = {"x86_64": 0x8664}

CLASSIFIERS = (
    "Development Status :: 4 - Beta",
    "Environment :: Console",
    "Intended Audience :: Developers",
    "Operating System :: MacOS",
    "Operating System :: Microsoft :: Windows",
    "Operating System :: POSIX :: Linux",
    "Programming Language :: Rust",
    "Topic :: Software Development :: Quality Assurance",
)


@dataclass(frozen=True)
class Project:
    version: str
    summary: str
    keywords: tuple[str, ...]
    license: str
    repository: str


def read_project(root: Path = ROOT) -> Project:
    workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]
    crate = tomllib.loads((root / "crates" / NAME / "Cargo.toml").read_text())["package"]
    return Project(
        version=workspace["version"],
        summary=crate["description"],
        keywords=tuple(crate["keywords"]),
        license=workspace["license"],
        repository=workspace["repository"],
    )


def check_binary(slot: Slot, data: bytes, label: str) -> None:
    if slot.kind in ("elf", "elf-static"):
        check_elf(slot, data, label)
    elif slot.kind == "macho":
        check_macho(slot, data, label)
    else:
        check_pe(slot, data, label)


def check_elf(slot: Slot, data: bytes, label: str) -> None:
    if data[:4] != b"\x7fELF" or data[4:6] != b"\x02\x01":
        raise BuildError(f"{label}: not a 64-bit little-endian ELF binary")
    (machine,) = struct.unpack_from("<H", data, 0x12)
    if machine != ELF_MACHINES[slot.arch]:
        raise BuildError(f"{label}: ELF machine {machine} is not {slot.arch}")
    if slot.kind == "elf-static" and elf_has_interpreter(data):
        raise BuildError(
            f"{label}: has a dynamic loader, but its wheel tags promise a static binary"
        )


def elf_has_interpreter(data: bytes) -> bool:
    (offset,) = struct.unpack_from("<Q", data, 0x20)
    size, count = struct.unpack_from("<HH", data, 0x36)
    return any(
        struct.unpack_from("<I", data, offset + index * size)[0] == 3 for index in range(count)
    )


def check_macho(slot: Slot, data: bytes, label: str) -> None:
    magic, cpu, _, _, commands = struct.unpack_from("<IIIII", data, 0)
    if magic != 0xFEEDFACF:
        raise BuildError(f"{label}: not a 64-bit Mach-O binary")
    if cpu != MACHO_CPUS[slot.arch]:
        raise BuildError(f"{label}: Mach-O CPU {cpu:#x} is not {slot.arch}")
    minos = macho_minos(data, commands)
    if minos is None:
        raise BuildError(f"{label}: no minimum macOS version recorded")
    if slot.minos is not None and minos > slot.minos:
        raise BuildError(
            f"{label}: needs macOS {minos[0]}.{minos[1]}, newer than its wheel tag allows"
        )


def macho_minos(data: bytes, commands: int) -> tuple[int, int] | None:
    offset = 32
    for _ in range(commands):
        command, size = struct.unpack_from("<II", data, offset)
        if command == 0x32:
            (version,) = struct.unpack_from("<I", data, offset + 12)
            return version >> 16, (version >> 8) & 0xFF
        if command == 0x24:
            (version,) = struct.unpack_from("<I", data, offset + 8)
            return version >> 16, (version >> 8) & 0xFF
        offset += size
    return None


def check_pe(slot: Slot, data: bytes, label: str) -> None:
    if data[:2] != b"MZ":
        raise BuildError(f"{label}: not a Windows executable")
    (offset,) = struct.unpack_from("<I", data, 0x3C)
    if data[offset : offset + 4] != b"PE\0\0":
        raise BuildError(f"{label}: no PE header")
    (machine,) = struct.unpack_from("<H", data, offset + 4)
    if machine != PE_MACHINES[slot.arch]:
        raise BuildError(f"{label}: PE machine {machine:#x} is not {slot.arch}")


def archive_files(path: Path) -> dict[str, bytes]:
    files: dict[str, bytes] = {}
    if path.name.endswith(".zip"):
        with zipfile.ZipFile(path) as archive:
            for info in archive.infolist():
                if not info.is_dir():
                    add_member(files, info.filename, archive.read(info), path)
    else:
        with tarfile.open(path, "r:gz") as archive:
            for member in archive:
                if member.isfile():
                    handle = archive.extractfile(member)
                    assert handle is not None
                    add_member(files, member.name, handle.read(), path)
    return files


def add_member(files: dict[str, bytes], name: str, data: bytes, archive: Path) -> None:
    parts = Path(name).parts
    if len(parts) > 2:
        return
    if parts[-1] in files:
        raise BuildError(f"{archive.name}: holds more than one {parts[-1]}")
    files[parts[-1]] = data


def pick(files: dict[str, bytes], name: str, archive: Path) -> bytes:
    if name not in files:
        raise BuildError(f"{archive.name}: no {name}")
    return files[name]


def absolute_links(markdown: str, repository: str, ref: str) -> str:
    blob = f"{repository}/blob/{ref}/"
    raw = repository.replace("https://github.com/", "https://raw.githubusercontent.com/")
    raw = f"{raw}/{ref}/"

    def target(url: str, image: bool) -> str:
        if re.match(r"[a-z][a-z0-9+.-]*:|#|/", url):
            return url
        return (raw if image else blob) + url.removeprefix("./")

    def link(match: re.Match[str]) -> str:
        bang, text, url = match.group(1), match.group(2), match.group(3)
        return f"{bang}[{text}]({target(url, bool(bang))})"

    def img(match: re.Match[str]) -> str:
        return f"{match.group(1)}{target(match.group(2), True)}{match.group(3)}"

    def outside_code(text: str) -> str:
        text = re.sub(r"(!?)\[([^\]]*)\]\(([^)\s]+)\)", link, text)
        return re.sub(r'(<img\b[^>]*\bsrc=")([^"]+)(")', img, text)

    lines = []
    fenced = False
    for line in markdown.splitlines(keepends=True):
        if line.lstrip().startswith("```"):
            fenced = not fenced
        if fenced or line.lstrip().startswith("```"):
            lines.append(line)
            continue
        spans = re.split(r"(`[^`]*`)", line)
        lines.append("".join(s if i % 2 else outside_code(s) for i, s in enumerate(spans)))
    return "".join(lines)


def metadata(project: Project, version: str, readme: str, ref: str) -> bytes:
    fields = [
        ("Metadata-Version", "2.4"),
        ("Name", NAME),
        ("Version", version),
        ("Summary", project.summary),
        ("Keywords", ",".join(project.keywords)),
        ("License-Expression", project.license),
        ("License-File", "LICENSE"),
        ("Project-URL", f"Homepage, {HOMEPAGE}"),
        ("Project-URL", f"Source, {project.repository}"),
        ("Project-URL", f"Changelog, {project.repository}/blob/{ref}/CHANGELOG.md"),
        ("Project-URL", f"Issues, {project.repository}/issues"),
        *(("Classifier", classifier) for classifier in CLASSIFIERS),
        ("Description-Content-Type", "text/markdown"),
    ]
    head = "".join(f"{key}: {value}\n" for key, value in fields)
    body = absolute_links(readme.replace("\r\n", "\n"), project.repository, ref)
    return f"{head}\n{body}".encode()


def record_hash(data: bytes) -> str:
    digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=")
    return f"sha256={digest.decode()}"


def write_wheel(
    slot: Slot,
    binary: bytes,
    project: Project,
    version: str,
    readme: str,
    license_text: bytes,
    ref: str,
    out: Path,
) -> Path:
    stem = f"{STEM}-{version}"
    info = f"{stem}.dist-info"
    wheel = "".join(
        [
            "Wheel-Version: 1.0\n",
            "Generator: bonsai-lint pypi/build_wheels.py\n",
            "Root-Is-Purelib: false\n",
            *(f"Tag: py3-none-{tag}\n" for tag in sorted(slot.tags)),
        ]
    )
    members = [
        (f"{stem}.data/scripts/{slot.executable}", binary, 0o755),
        (f"{info}/METADATA", metadata(project, version, readme, ref), 0o644),
        (f"{info}/WHEEL", wheel.encode(), 0o644),
        (f"{info}/licenses/LICENSE", license_text.replace(b"\r\n", b"\n"), 0o644),
    ]
    rows = [(path, record_hash(data), str(len(data))) for path, data, _ in members]
    rows.append((f"{info}/RECORD", "", ""))
    record = io.StringIO(newline="")
    csv.writer(record, lineterminator="\n").writerows(rows)
    members.append((f"{info}/RECORD", record.getvalue().encode(), 0o644))

    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{stem}-py3-none-{slot.platform}.whl"
    with zipfile.ZipFile(path, "w") as archive:
        for name, data, mode in members:
            entry = zipfile.ZipInfo(name, date_time=ZIP_EPOCH)
            entry.create_system = 3
            entry.external_attr = (stat.S_IFREG | mode) << 16
            entry.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(entry, data, compresslevel=9)
    return path


def verify_wheel(path: Path, slot: Slot, binary: bytes) -> None:
    with zipfile.ZipFile(path) as archive:
        names = [entry.filename for entry in archive.infolist()]
        record_name = next(name for name in names if name.endswith(".dist-info/RECORD"))
        rows = list(csv.reader(io.StringIO(archive.read(record_name).decode())))
        if sorted(row[0] for row in rows) != sorted(names):
            raise BuildError(f"{path.name}: RECORD does not list exactly the wheel's files")
        for name, digest, size in rows:
            if name == record_name:
                continue
            data = archive.read(name)
            if digest != record_hash(data) or size != str(len(data)):
                raise BuildError(f"{path.name}: RECORD is wrong for {name}")
        script = next(name for name in names if name.endswith(f".data/scripts/{slot.executable}"))
        if archive.read(script) != binary:
            raise BuildError(f"{path.name}: the binary differs from the release archive's")
        if archive.getinfo(script).external_attr >> 16 != stat.S_IFREG | 0o755:
            raise BuildError(f"{path.name}: the binary is not marked executable")


def release_artifacts(manifest: dict, tag: str) -> tuple[str, dict[str, tuple[str, str]]]:
    if manifest.get("announcement_tag") != tag:
        raise BuildError(f"the manifest announces {manifest.get('announcement_tag')}, not {tag}")
    release = next((r for r in manifest.get("releases", []) if r.get("app_name") == NAME), None)
    if release is None or f"v{release['app_version']}" != tag:
        raise BuildError(f"the manifest's {NAME} version does not match {tag}")
    artifacts = {}
    for name, artifact in manifest.get("artifacts", {}).items():
        if artifact.get("kind") == "executable-zip":
            for triple in artifact.get("target_triples", []):
                artifacts[triple] = (name, artifact["checksums"]["sha256"])
    missing = [triple for triple in RELEASE_SLOTS if triple not in artifacts]
    if missing:
        raise BuildError(f"the manifest has no archive for {', '.join(missing)}")
    return release["app_version"], artifacts


def build_release(
    manifest_path: Path, archives: Path, tag: str, out: Path, suffix: str = ""
) -> list[tuple[Path, str]]:
    if suffix and not re.fullmatch(r"\.dev\d+", suffix):
        raise BuildError(f"a version suffix must be .devN, not {suffix}")
    version, artifacts = release_artifacts(json.loads(manifest_path.read_text()), tag)
    project = read_project()

    contents = {}
    for triple, slot in RELEASE_SLOTS.items():
        name, expected = artifacts[triple]
        wanted = ".zip" if slot.kind == "pe" else ".tar.gz"
        if not name.endswith(wanted):
            raise BuildError(f"{name}: expected a {wanted} archive")
        path = archives / name
        if not path.is_file():
            raise BuildError(f"{name}: not found in {archives}")
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise BuildError(f"{name}: sha256 differs from the release manifest")
        contents[triple] = (path, archive_files(path))

    docs_path, docs = contents[DOCS_TARGET]
    readme = pick(docs, "README.md", docs_path).decode()
    license_text = pick(docs, "LICENSE", docs_path)

    built = []
    for triple, slot in RELEASE_SLOTS.items():
        path, files = contents[triple]
        binary = pick(files, slot.executable, path)
        check_binary(slot, binary, path.name)
        wheel = write_wheel(
            slot, binary, project, version + suffix, readme, license_text, tag, out
        )
        verify_wheel(wheel, slot, binary)
        built.append((wheel, hashlib.sha256(binary).hexdigest()))
    return built


def build_binary(binary_path: Path, target: str, out: Path, root: Path = ROOT) -> Path:
    slot = LOCAL_SLOTS.get(target)
    if slot is None:
        raise BuildError(f"no wheel platform for {target}")
    binary = binary_path.read_bytes()
    check_binary(slot, binary, str(binary_path))
    project = read_project(root)
    readme = (root / "README.md").read_text()
    license_text = (root / "LICENSE").read_bytes()
    ref = f"v{project.version}"
    wheel = write_wheel(slot, binary, project, project.version, readme, license_text, ref, out)
    verify_wheel(wheel, slot, binary)
    return wheel


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    modes = parser.add_subparsers(dest="mode", required=True)
    release = modes.add_parser("release", help="the five published wheels from a GitHub Release")
    release.add_argument("--manifest", type=Path, required=True)
    release.add_argument("--archives", type=Path, required=True)
    release.add_argument("--tag", required=True)
    release.add_argument("--out", type=Path, required=True)
    release.add_argument("--version-suffix", default="")
    local = modes.add_parser("binary", help="one local build, for testing the hook")
    local.add_argument("--binary", type=Path, required=True)
    local.add_argument("--target", required=True, choices=sorted(LOCAL_SLOTS))
    local.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)

    try:
        if args.mode == "release":
            for wheel, digest in build_release(
                args.manifest, args.archives, args.tag, args.out, args.version_suffix
            ):
                print(f"{wheel.name}  binary sha256 {digest}")
        else:
            print(build_binary(args.binary, args.target, args.out).name)
    except BuildError as error:
        print(f"build_wheels: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
