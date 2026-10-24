#!/usr/bin/env python3
"""Network-free end-to-end version/Vanilla planning parity via shared Zig cache fixtures."""
import argparse
import hashlib
import io
import zipfile
import json
from pathlib import Path
import subprocess
import tempfile
import time


def run(binary, args, cwd, data=None):
    return subprocess.run([str(binary), *args], input=data, capture_output=True, cwd=cwd, timeout=10)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--zig', type=Path, required=True)
    parser.add_argument('--rust', type=Path, required=True)
    options = parser.parse_args()
    zig, rust = options.zig.resolve(), options.rust.resolve()
    count = 0
    with tempfile.TemporaryDirectory(prefix='jmcl-version-parity-') as directory:
        root = Path(directory)
        cache = root / '.jmclcore' / 'metadata-cache'
        cache.mkdir(parents=True)
        metadata = {'id': 'demo', 'type': 'release', 'mainClass': 'example.Main', 'libraries': [],
                    'downloads': {'client': {'sha1': '1' * 40, 'size': 12,
                                            'url': 'https://piston-data.mojang.com/client'}},
                    'javaVersion': {'component': 'runtime', 'majorVersion': 17}}
        payload = json.dumps(metadata, separators=(',', ':'))
        manifest = {'latest': {'release': 'demo', 'snapshot': 'demo'}, 'versions': [
            {'id': 'demo', 'type': 'release', 'url': 'https://piston-meta.mojang.com/demo',
             'time': '2026-01-01', 'releaseTime': '2026-01-01', 'sha1': hashlib.sha1(payload.encode()).hexdigest()}]}
        fixtures = {
            'https://piston-meta.mojang.com/mc/game/version_manifest_v2.json': json.dumps(manifest),
            'https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json': json.dumps(manifest),
            'https://piston-meta.mojang.com/demo': payload,
            'https://bmclapi2.bangbang93.com/demo': payload,
        }
        profile = {'id': 'fabric-loader-0.16.10-demo', 'inheritsFrom': 'demo',
                   'mainClass': 'net.fabricmc.loader.impl.launch.knot.KnotClient',
                   'arguments': {'game': [], 'jvm': ['-Dfabric=true']},
                   'libraries': [{'name': 'net.fabricmc:fabric-loader:0.16.10',
                                  'url': 'https://maven.fabricmc.net/'}]}
        artifact = 'https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar'
        fixtures.update({
            'https://meta.fabricmc.net/v2/versions/loader/demo/0.16.10/profile/json': json.dumps(profile),
            artifact + '.sha1': '2' * 40 + '  fabric-loader.jar\n',
            'head:' + artifact: '30',
        })
        neo_version = {'id': 'neoforge-1', 'inheritsFrom': 'demo', 'mainClass': 'Neo.Main',
                       'arguments': {'game': ['neo'], 'jvm': []}, 'libraries': []}
        neo_install = {'spec': 1, 'version': 'neoforge-1', 'minecraft': 'demo',
                       'data': {'PATCHED': {'client': '[net.neoforged:neoforge:1:client]'}},
                       'libraries': [{'name': 'example:processor:1', 'downloads': {'artifact': {
                           'path': 'ignored', 'sha1': '3' * 40, 'size': 33,
                           'url': 'https://example.invalid/processor.jar'}}}]}
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as jar:
            jar.writestr('version.json', json.dumps(neo_version))
            jar.writestr('install_profile.json', json.dumps(neo_install))
        fixtures['https://maven.neoforged.net/releases/net/neoforged/neoforge/1/neoforge-1-installer.jar'] = list(archive.getvalue())
        def installer(loader, install, version=None):
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as jar:
                jar.writestr('install_profile.json', json.dumps(install))
                if version is not None:
                    jar.writestr('version.json', json.dumps(version))
            fixtures[f'https://maven.minecraftforge.net/net/minecraftforge/forge/demo-{loader}/forge-demo-{loader}-installer.jar'] = list(archive.getvalue())
        modern_install = {'spec': 1, 'version': 'demo-forge-2', 'minecraft': 'demo',
                          'data': {'PATCHED': {'client': '[net.minecraftforge:forge:demo-2:client]'}},
                          'processors': [{'jar': 'example:processor:1'}], 'libraries': neo_install['libraries']}
        modern_version = {'id': 'demo-forge-2', 'inheritsFrom': 'demo', 'mainClass': 'Forge.Main',
                          'libraries': [{'name': 'net.minecraftforge:forge:demo-2:client', 'downloads': {'artifact': {
                              'path': 'ignored', 'sha1': '4' * 40, 'size': 44, 'url': ''}}}]}
        installer('2', modern_install, modern_version)
        processorless_install = {'spec': 1, 'version': 'demo-forge-3', 'minecraft': 'demo', 'data': {}}
        processorless_version = {'id': 'demo-forge-3', 'inheritsFrom': 'demo', 'mainClass': 'Forge.Main',
                                'minecraftArguments': '--legacy true',
                                'libraries': [{'name': 'net.minecraftforge:forge:demo-3', 'downloads': {'artifact': {
                                    'path': 'ignored', 'sha1': '5' * 40, 'size': 55, 'url': ''}}}]}
        installer('3', processorless_install, processorless_version)
        legacy_install = {'install': {'path': 'net.minecraftforge:forge:demo-4', 'minecraft': 'demo'},
                          'versionInfo': {'id': 'demo-Forge4', 'mainClass': 'Legacy.Main', 'minecraftArguments': '--legacy true',
                                          'libraries': [{'name': 'net.minecraftforge:forge:demo-4',
                                                         'url': 'http://files.minecraftforge.net/maven/', 'checksums': ['6' * 40]}]}}
        installer('4', legacy_install)
        fixtures['head:https://maven.minecraftforge.net/net/minecraftforge/forge/demo-4/forge-demo-4-universal.jar'] = '66'
        for url, body in fixtures.items():
            (cache / (hashlib.sha1(url.encode()).hexdigest() + '.json')).write_text(json.dumps(
                {'v': 1, 'url': url, 'fetched_at': int(time.time()), 'payload': body}), encoding='utf-8')
        cases = [('version.list', None), ('version.list', {}), ('version.list', {'type': 'snapshot'}),
                 ('version.list', {'limit': '1.0'}), ('version.get', {'id': 'missing'}),
                 ('version.get', {'id': 'demo'}), ('version.resolve', {'id': 'demo'}),
                 ('install.plan', {'id': 'demo', 'os': 'linux', 'arch': 'x86_64'})]
        for method in ['version.list', 'version.get', 'version.resolve', 'install.plan']:
            cases += [(method, value) for value in [[], '', 5, {'id': ''}, {'id': 5},
                       {'id': 'demo', 'source': 'bad'}, {'id': 'demo', 'source': None}]]
        cases += [('version.list', {'limit': value}) for value in [0, 1001, -1, None, 1.5]]
        cases += [('version.resolve', {'id': 'demo', 'fabric_loader': 'a', 'forge_version': 'b'}),
                  ('install.plan', {'id': 'demo', 'fabric_loader': 'a', 'forge_version': 'b'}),
                  ('install.plan', {'id': 'demo', 'arch': 'amd64'})]
        cases += [(method, {'id': 'demo', 'source': 'bmclapi'}) for method in
                  ['version.list', 'version.get', 'version.resolve', 'install.plan']]
        cases += [(method, {'id': 'demo', 'fabric_loader': '0.16.10', 'source': source})
                  for source in ['official', 'bmclapi'] for method in ['version.resolve', 'install.plan']]
        cases += [(method, {'id': 'demo', 'neoforge_version': '1', 'source': source})
                  for source in ['official', 'bmclapi'] for method in ['version.resolve', 'install.plan']]
        cases += [(method, {'id': 'demo', 'forge_version': loader, 'source': source})
                  for loader in ['2', '3', '4'] for source in ['official', 'bmclapi']
                  for method in ['version.resolve', 'install.plan']]
        # Omit params for None; wire null means the same absent optional value in Zig.
        for index, (method, params) in enumerate(cases):
            request = {'protocol': 1, 'id': str(index), 'method': method}
            if params is not None:
                request['params'] = params
            data = (json.dumps(request) + '\n').encode()
            a, b = run(zig, ['rpc'], root, data), run(rust, ['rpc'], root, data)
            assert (a.returncode, a.stdout, a.stderr) == (b.returncode, b.stdout, b.stderr), (
                request, a.stdout, b.stdout, a.stderr, b.stderr)
            count += 1
        commands = [
            ['version', 'list'], ['version', 'list', '--json'],
            ['version', 'list', '--type', 'snapshot', '--limit', '1', '--json'],
            ['version', 'list', '--limit', '0'], ['version', 'list', '--limit', '1_0'],
            ['version', 'show', 'demo'], ['version', 'show', 'demo', '--json'],
            ['version', 'show', 'missing'], ['version', 'show'],
            ['version', 'show', 'demo', '--bad'],
            ['install', 'plan', 'demo'], ['install', 'plan', 'demo', '--json'],
            ['install', 'plan', 'demo', '--arch', 'amd64', '--os', 'linux', '--json'],
            ['install', 'plan'], ['install', 'plan', 'demo', '--os', 'bad'],
        ]
        commands += [command + ['--source', 'bmclapi'] for command in
                     [['version', 'list'], ['version', 'show', 'demo'], ['install', 'plan', 'demo']]]
        commands += [['install', 'plan', 'demo', '--fabric-loader', '0.16.10'] + extra
                     for extra in [[], ['--json'], ['--source', 'bmclapi'], ['--source', 'bmclapi', '--json']]]
        commands += [['install', 'plan', 'demo', '--neoforge-version', '1'] + extra
                     for extra in [[], ['--json'], ['--source', 'bmclapi'], ['--source', 'bmclapi', '--json']]]
        commands += [['install', 'plan', 'demo', '--forge-version', loader] + extra
                     for loader in ['2', '3', '4']
                     for extra in [[], ['--json'], ['--source', 'bmclapi'], ['--source', 'bmclapi', '--json']]]
        for command in commands:
            a, b = run(zig, command, root), run(rust, command, root)
            assert (a.returncode, a.stdout, a.stderr) == (b.returncode, b.stdout, b.stderr), (
                command, a.stdout, b.stdout, a.stderr, b.stderr)
            count += 1
    print(f'{count} network-free RPC/CLI version cases matched exactly; Vanilla, Fabric, NeoForge and three Forge profile families covered')


if __name__ == '__main__':
    main()
