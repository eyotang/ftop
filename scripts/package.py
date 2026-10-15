#!/usr/bin/env python3
"""Build a native package on the target OS. Does not sign, upload, or publish."""
import pathlib
import platform
import shutil
import subprocess
import tempfile
import tomllib
import zipfile

root = pathlib.Path(__file__).resolve().parent.parent
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
system = platform.system()
arch = {'AMD64': 'x86_64', 'arm64': 'aarch64'}.get(platform.machine(), platform.machine())
subprocess.run(['cargo', 'build', '--release', '--locked'], cwd=root, check=True)
out = root / 'dist'
out.mkdir(exist_ok=True)
name = f'ftop-{version}-{system.lower()}-{arch}'
with tempfile.TemporaryDirectory(prefix='ftop-package-') as temporary:
    stage = pathlib.Path(temporary)
    if system == 'Darwin':
        import plistlib
        bundle = stage / 'ftop.app' / 'Contents'
        (bundle / 'MacOS').mkdir(parents=True)
        (bundle / 'Resources').mkdir()
        shutil.copy2(root / 'assets/AppIcon.icns', bundle / 'Resources/AppIcon.icns')
        shutil.copy2(root / 'LICENSE', bundle / 'Resources/LICENSE')
        shutil.copy2(root / 'target/release/ftop', bundle / 'MacOS/ftop')
        with (bundle / 'Info.plist').open('wb') as target:
            plistlib.dump({'CFBundleName': 'ftop', 'CFBundleIconFile': 'AppIcon.icns', 'CFBundleIdentifier': 'io.github.eyotang.ftop', 'CFBundleExecutable': 'ftop', 'CFBundleVersion': version, 'CFBundleShortVersionString': version, 'CFBundlePackageType': 'APPL', 'NSHighResolutionCapable': True, 'LSMinimumSystemVersion': '14.0'}, target)
        subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(stage / 'ftop.app'), str(out / f'{name}.zip')], check=True)
        # Finder exposes the Applications link as a drag-and-drop install target.
        (stage / 'Applications').symlink_to('/Applications', target_is_directory=True)
        (stage / 'Install.txt').write_text('Drag ftop.app onto Applications to install.\n将 ftop.app 拖到 Applications（应用程序）即可安装。\n')
        dmg = out / f'{name}.dmg'
        subprocess.run(['hdiutil', 'create', '-volname', 'ftop', '-srcfolder', str(stage), '-format', 'UDZO', '-ov', str(dmg)], check=True)
        subprocess.run(['hdiutil', 'verify', str(dmg)], check=True)
        # Mount the actual image read-only and validate the install target and icon.
        import plistlib
        mounted = plistlib.loads(subprocess.check_output(['hdiutil', 'attach', '-readonly', '-nobrowse', '-plist', str(dmg)]))
        volume = next(pathlib.Path(e['mount-point']) for e in mounted['system-entities'] if 'mount-point' in e)
        try:
            assert (volume / 'Applications').is_symlink()
            assert (volume / 'Applications').readlink() == pathlib.Path('/Applications')
            assert (volume / 'ftop.app/Contents/Resources/AppIcon.icns').stat().st_size > 0
            info = plistlib.loads((volume / 'ftop.app/Contents/Info.plist').read_bytes())
            assert info['CFBundleIconFile'] == 'AppIcon.icns'
        finally:
            subprocess.run(['hdiutil', 'detach', str(volume)], check=True)

    elif system == 'Windows':
        # Verify the actual PE header: IMAGE_SUBSYSTEM_WINDOWS_GUI = 2.
        import struct
        executable = (root / 'target/release/ftop.exe').read_bytes()
        pe = struct.unpack_from('<I', executable, 0x3c)[0]
        assert executable[pe:pe + 4] == b'PE\0\0'
        subsystem = struct.unpack_from('<H', executable, pe + 24 + 68)[0]
        assert subsystem == 2, f'Expected Windows GUI subsystem, got {subsystem}'
        with zipfile.ZipFile(out / f'{name}-portable.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.write(root / 'target/release/ftop.exe', 'ftop.exe')
            archive.write(root / 'LICENSE', 'LICENSE')
    elif system == 'Linux':
        (stage / 'usr/bin').mkdir(parents=True)
        (stage / 'usr/share/applications').mkdir(parents=True)
        (stage / 'usr/share/doc/ftop').mkdir(parents=True)
        shutil.copy2(root / 'LICENSE', stage / 'usr/share/doc/ftop/copyright')
        (stage / 'usr/share/icons/hicolor/256x256/apps').mkdir(parents=True)
        shutil.copy2(root / 'target/release/ftop', stage / 'usr/bin/ftop')
        shutil.copy2(root / 'assets/icon.png', stage / 'usr/share/icons/hicolor/256x256/apps/ftop.png')
        (stage / 'usr/share/applications/ftop.desktop').write_text('[Desktop Entry]\nType=Application\nName=ftop\nComment=Quiet system monitor\nExec=ftop\nIcon=ftop\nStartupWMClass=ftop\nTerminal=false\nCategories=System;Monitor;\n')
        (stage / 'DEBIAN').mkdir()
        deb_arch = {'x86_64':'amd64', 'aarch64':'arm64'}.get(arch)
        if not deb_arch:
            raise SystemExit(f'Unsupported Debian architecture: {arch}')
        (stage / 'DEBIAN/control').write_text(f'Package: ftop\nVersion: {version}\nArchitecture: {deb_arch}\nMaintainer: eyotang <2398680+eyotang@users.noreply.github.com>\nDescription: GPUI Kit native system monitor\nDepends: libc6 (>= {platform.libc_ver()[1]}), libxcb1, libxkbcommon0, libxkbcommon-x11-0, libfreetype6, libfontconfig1, libwayland-client0, libvulkan1\n')
        subprocess.run(['dpkg-deb', '--root-owner-group', '--build', str(stage), str(out / f'{name}.deb')], check=True)
    else:
        raise SystemExit(f'Unsupported platform: {system}')
print(f'Built native {system} package in {out}; verify on this OS before release.')
