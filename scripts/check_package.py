"""Verify the Windows release contains engines and requests elevation."""

from pathlib import Path
import sys

import pefile
from PyInstaller.archive.readers import CArchiveReader


def check(path: Path):
    pe = pefile.PE(str(path))
    manifests = []
    for resource in pe.DIRECTORY_ENTRY_RESOURCE.entries:
        if resource.id != 24:  # RT_MANIFEST
            continue
        for name in resource.directory.entries:
            for language in name.directory.entries:
                entry = language.data.struct
                manifests.append(pe.get_data(entry.OffsetToData, entry.Size))
    if not any(b"requireAdministrator" in manifest for manifest in manifests):
        raise AssertionError("EXE не запрашивает права администратора")

    archive = CArchiveReader(str(path))
    bundled = {name.replace("\\", "/") for name in archive.toc}
    required = {"bin/sing-box.exe", "bin/xray.exe", "bin/libcronet.dll", "bin/wintun.dll"}
    missing = required - bundled
    if missing:
        raise AssertionError(f"В EXE отсутствуют файлы: {sorted(missing)}")
    modules = archive.open_embedded_archive("PYZ.pyz").toc
    if "pystray._win32" not in modules:
        raise AssertionError("В EXE отсутствует Windows tray backend")
    print("Manifest requireAdministrator, tray backend and bundled engines: OK")


if __name__ == "__main__":
    check(Path(sys.argv[1]))
