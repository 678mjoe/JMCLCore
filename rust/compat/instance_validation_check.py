#!/usr/bin/env python3
"""Cache-only instance.create RPC/CLI oracle: loaders, sources, defaults and error order.

All metadata/installer/sidecar inputs are synthetic; no game, installer or network
is executed. The fixture shape is shared with version_check.py's pinned cache corpus.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import zipfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--zig', type=Path, required=True)
    parser.add_argument('--rust', type=Path, required=True)
    options = parser.parse_args()
    binaries = [options.zig.resolve(), options.rust.resolve()]
    count = 0
    with tempfile.TemporaryDirectory(prefix='jmcl-instance-validation-') as directory:
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
        def compare(params, cli=False, expected=None):
            nonlocal count
            outputs = []
            for binary in binaries:
                instances = root / 'instances'
                if instances.exists():
                    shutil.rmtree(instances)
                if cli:
                    args, data = params, None
                else:
                    args = ['rpc']
                    data = (json.dumps({'protocol': 1, 'id': 'create',
                                        'method': 'instance.create', 'params': params}) + '\n').encode()
                result = subprocess.run([str(binary), *args], cwd=root, input=data,
                                        capture_output=True, timeout=10)
                disk = sorted((str(p.relative_to(instances)), p.read_bytes())
                              for p in instances.rglob('*') if p.is_file()) if instances.exists() else None
                outputs.append((result.returncode, result.stdout, result.stderr, disk))
            assert outputs[0] == outputs[1], (params, outputs)
            if expected is not None:
                assert expected.encode() in outputs[0][1] + outputs[0][2], (params, outputs)
                assert outputs[0][3] is None, ('failure must not create root', params, outputs)
            else:
                assert outputs[0][0] == 0 and outputs[0][3], (params, outputs)
            count += 1

        selections = [{}, {'fabric_loader': '0.16.10'}, {'neoforge_version': '1'}]
        selections += [{'forge_version': loader} for loader in ['2', '3', '4']]
        for source in ['official', 'bmclapi']:
            for loader in selections:
                base = dict(directory='instances', id='demo', version_id='demo', source=source, **loader)
                compare(base)
                compare(dict(base, validate=True))
                args = ['instance', 'create', 'demo', '--version', 'demo', '--source', source]
                for key, value in loader.items():
                    args += ['--' + key.replace('_', '-'), value]
                compare(args, cli=True)

        # Cached missing version fails before root creation and before manifest name validation.
        for source in ['official', 'bmclapi']:
            for loader in selections:
                p = dict(directory='instances', id='demo', version_id='missing', source=source,
                         name='', **loader)
                compare(p, expected='VERSION_RESOLUTION_FAILED')
            compare(['instance', 'create', 'demo', '--version', 'missing', '--source', source],
                    cli=True, expected='failed to resolve Minecraft version: VersionNotFound')
        for changes, error in [({'id': '../bad'}, 'the instance id is invalid'),
                               ({'source': 'bad'}, 'source must be official or bmclapi'),
                               ({'fabric_loader': 'x', 'forge_version': 'y'}, 'only one loader field may be set')]:
            compare(dict(directory='instances', id='demo', version_id='missing') | changes,
                    expected=error)

        # Resolution failure takes precedence over an unusable instances root.
        (root / 'blocked').write_text('not a directory')
        compare(dict(directory='blocked', id='demo', version_id='missing'),
                expected='VERSION_RESOLUTION_FAILED')
        compare(dict(directory='blocked', id='demo', version_id='demo'),
                expected='INSTANCES_ROOT_ERROR')
        compare(['instance', 'create', 'demo', '--version', 'missing',
                 '--instances-directory', 'blocked'], cli=True,
                expected='failed to resolve Minecraft version: VersionNotFound')
        compare(['instance', 'create', 'demo', '--version', 'demo',
                 '--instances-directory', 'blocked'], cli=True,
                expected='failed to open instances directory:')

        # No-validation must not even consult missing metadata, including unknown loaders.
        for loader in [{}, {'fabric_loader': 'uncached'}, {'neoforge_version': 'uncached'}, {'forge_version': 'uncached'}]:
            compare(dict(directory='instances', id='demo', version_id='uncached', validate=False, **loader))
            args = ['instance', 'create', 'demo', '--version', 'uncached', '--no-validate']
            for key, value in loader.items():
                args += ['--' + key.replace('_', '-'), value]
            compare(args, cli=True)

        # A syntactically valid mirror manifest lacking demo forces official fallback,
        # without treating corrupted cache as a reason to fetch from the network.
        url = 'https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json'
        (cache / (hashlib.sha1(url.encode()).hexdigest() + '.json')).write_text(json.dumps(
            {'v': 1, 'url': url, 'fetched_at': int(time.time()),
             'payload': json.dumps({'latest': {'release': 'absent', 'snapshot': 'absent'}, 'versions': []})}))
        for loader in selections:
            compare(dict(directory='instances', id='demo', version_id='demo', source='bmclapi', **loader))
    print(f'{count} cached instance validation RPC/CLI cases matched stdout/stderr/status/disk exactly')


if __name__ == '__main__':
    main()
