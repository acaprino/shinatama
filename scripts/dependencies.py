"""Locked Windows dependency inventory and deterministic third-party notices.

Standard library only. This is a metadata/text completeness check, not legal signoff.
"""
from __future__ import annotations

import argparse
import hashlib
import html
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tomllib
from urllib.parse import urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parents[1]
TARGET = 'x86_64-pc-windows-msvc'
NOTICE = 'THIRD_PARTY_NOTICES.md'
POLICY = 'docs/implementation/dependency-policy.md'
SKIP_DIRECTORIES = {'.git', '.agents', '.codex', '.daodan', '.jcode', '.prototype-evidence', '.verification', '__pycache__', 'node_modules', 'target', 'dist', 'verify'}
EXACT_VERSION = re.compile(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?\Z')
# A release of the suite's own packages (Imago, Sigrid): the asset of a GitHub Release, whose version is in its name and in the lock.
SUITE_RELEASE = re.compile(r'https://github\.com/acaprino/(imago|sigrid)/releases/download/v(\d+\.\d+\.\d+)/acaprino-\1-\2\.tgz\Z')
PINNED_NPM_GIT = re.compile(r'git\+https://[A-Za-z0-9.-]+/[A-Za-z0-9._/-]+\.git#[0-9a-f]{40}\Z')
KNOWN_LICENSES = {'MIT', 'MIT-0', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', 'ISC', '0BSD', 'CC0-1.0', 'Unlicense', 'Zlib', 'Unicode-3.0', 'BSL-1.0', 'MPL-2.0'}
REVIEW_LICENSES = {'MPL-2.0'}
NPM_LICENSE_WRAPPERS = {'@rolldown/binding-win32-x64-msvc':'rolldown', '@tauri-apps/cli-win32-x64-msvc':'@tauri-apps/cli'}
DEPENDENCY_SECTIONS = ('dependencies', 'dev-dependencies', 'build-dependencies')
SIDECAR_MIRROR = Path('sidecar/source-build/prepared')
SIDECAR_PIN = Path('sidecar/source-build/provenance.lock.json')
SIDECAR_MANIFESTS = frozenset({'package.json', 'kernel/package.json', 'pi-subagents/package.json'})
SIDECAR_MAX_ENTRIES = 1024


def sha256(data: str | bytes) -> str:
    return hashlib.sha256(data.encode('utf-8') if isinstance(data, str) else data).hexdigest()


def read_json(path: Path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def checked_mirror_path(root: Path, relative: Path, directory=False) -> Path:
    """Static build metadata only; reject every reparse component before reading."""
    if relative.is_absolute() or not relative.parts or any(part in {'.', '..'} or ':' in part or '\\' in part for part in relative.parts):
        raise ValueError('Invalid prepared mirror path')
    path = root
    for index, part in enumerate(relative.parts):
        path /= part
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0):
            raise ValueError('Prepared mirror reparse point is forbidden')
        expected_directory = index < len(relative.parts)-1 or directory
        if not (stat.S_ISDIR(info.st_mode) if expected_directory else stat.S_ISREG(info.st_mode)):
            raise ValueError('Prepared mirror entry type is invalid')
    if not path.resolve().is_relative_to(root.resolve()):
        raise ValueError('Prepared mirror escapes the repository')
    return path


def bounded_mirror_bytes(root: Path, relative: Path, maximum: int) -> bytes:
    path = checked_mirror_path(root, relative)
    before = path.stat()
    if before.st_size > maximum:
        raise ValueError('Prepared mirror metadata exceeds its byte bound')
    with path.open('rb') as source:
        opened = os.fstat(source.fileno())
        if (before.st_dev, before.st_ino, before.st_size) != (opened.st_dev, opened.st_ino, opened.st_size):
            raise ValueError('Prepared mirror entry changed while opening')
        checked_mirror_path(root, relative)
        data = source.read(maximum+1)
    if len(data) != before.st_size or len(data) > maximum:
        raise ValueError('Prepared mirror metadata changed or exceeds its byte bound')
    return data


def validate_prepared_sidecar(root: Path) -> tuple[bool, list[str]]:
    """Only three hash-pinned upstream manifests may share this source mirror."""
    try:
        prepared = checked_mirror_path(root, SIDECAR_MIRROR, directory=True)
    except FileNotFoundError:
        return False, []  # The ignored prepared runtime is optional in a clean checkout.
    except (OSError, ValueError):
        return False, ['Prepared sidecar mirror is not a contained regular directory']
    try:
        pin = json.loads(bounded_mirror_bytes(root, SIDECAR_PIN, 256*1024).decode('utf-8-sig'))
        if pin.get('formatVersion') != 1 or pin.get('packageName') != 'billion-context':
            raise ValueError('Invalid prepared source identity')
        entries = []
        for section in ('sourceFiles', 'artifactFiles'):
            group = pin.get(section)
            if not isinstance(group, list) or len(group) > SIDECAR_MAX_ENTRIES-len(entries):
                raise ValueError('Prepared source file list exceeds its bound')
            entries.extend(group)
        pinned = {}
        for entry in entries:
            name = entry['path']
            if not isinstance(name, str) or not name or len(name) > 4096 or '\\' in name or ':' in name or '\x00' in name or any(part in {'', '.', '..'} for part in name.split('/')):
                raise ValueError('Invalid prepared source relative path')
            if name in pinned or not isinstance(entry['bytes'], int) or not 0 <= entry['bytes'] <= 32*1024*1024 or not isinstance(entry['sha256'], str) or not re.fullmatch('[0-9a-f]{64}', entry['sha256']):
                raise ValueError('Invalid or duplicate prepared source fingerprint')
            pinned[name] = entry
        for name in sorted(SIDECAR_MANIFESTS):
            entry = pinned[name]
            body = bounded_mirror_bytes(root, SIDECAR_MIRROR/name, 256*1024)
            if len(body) != entry['bytes'] or sha256(body) != entry['sha256']:
                raise ValueError('Prepared upstream manifest fingerprint changed')
        # Unlike a blanket directory exclusion, this walk still rejects unknown
        # manifests. Include dist; node_modules retains the existing dependency
        # exclusion and is not treated as another first-party package declaration.
        pending = [(prepared, 0)]
        count = 0
        while pending:
            directory, depth = pending.pop()
            if depth > 32:
                raise ValueError('Prepared mirror exceeds its depth bound')
            with os.scandir(directory) as children:
                for child in children:
                    count += 1
                    if count > SIDECAR_MAX_ENTRIES:
                        raise ValueError('Prepared mirror exceeds its entry bound')
                    info = child.stat(follow_symlinks=False)
                    if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0):
                        raise ValueError('Prepared mirror reparse point is forbidden')
                    path = Path(child.path)
                    if stat.S_ISDIR(info.st_mode):
                        if child.name != 'node_modules':
                            pending.append((path, depth+1))
                    elif child.name == 'package.json' and path.relative_to(prepared).as_posix() not in SIDECAR_MANIFESTS:
                        return False, [f'Second npm manifest is forbidden: {path.relative_to(root).as_posix()}']
        return True, []
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        return False, ['Prepared sidecar manifest provenance failed: bounded pin, path, file or SHA-256 validation']


def clean_url(value) -> str | None:
    if isinstance(value, dict):
        value = value.get('url')
    if not isinstance(value, str):
        return None
    value = value.removeprefix('git+')
    try:
        url = urlsplit(value)
        if url.scheme not in {'http', 'https'} or not url.hostname:
            return None
        # Never expose embedded credentials, fragments or query strings.
        return urlunsplit((url.scheme, url.hostname, url.path, '', ''))
    except ValueError:
        return None


def license_policy(expression: str | None) -> tuple[str, list[str]]:
    if not isinstance(expression, str) or not expression.strip():
        return 'error', ['Missing license declaration']
    # Cargo still contains historical slash-separated OR expressions.
    normalized = re.sub(r'\s*/\s*', ' OR ', expression.strip())
    tokens = re.findall(r'[A-Za-z0-9][A-Za-z0-9.+-]*|[()]', normalized)
    if ''.join(tokens) != re.sub(r'\s+', '', normalized):
        return 'error', [f'Unrecognized SPDX expression: {expression}']
    position = 0
    identifiers = set()

    def atom():
        nonlocal position
        if position >= len(tokens):
            raise ValueError('missing operand')
        token = tokens[position]
        position += 1
        if token == '(':
            disjunction()
            if position >= len(tokens) or tokens[position] != ')':
                raise ValueError('unbalanced parentheses')
            position += 1
        elif token in KNOWN_LICENSES:
            identifiers.add(token)
        else:
            raise ValueError(f'unreviewed identifier {token}')

    def conjunction():
        nonlocal position
        atom()
        while position < len(tokens) and tokens[position] == 'AND':
            position += 1
            atom()

    def disjunction():
        nonlocal position
        conjunction()
        while position < len(tokens) and tokens[position] == 'OR':
            position += 1
            conjunction()
    try:
        disjunction()
        if position != len(tokens):
            raise ValueError('unexpected token')
    except ValueError as error:
        return 'error', [f'Unrecognized SPDX expression {expression}: {error}']
    if identifiers & REVIEW_LICENSES:
        return 'release-review-required', ['MPL-2.0 obligations and source availability need release review; this is not MIT approval']
    return 'recognized', []


def validate_npm(root: Path) -> list[str]:
    errors = []
    manifest = read_json(root / 'package.json')
    lock = read_json(root / 'package-lock.json')
    if lock.get('lockfileVersion') != 3:
        errors.append('package-lock.json: expected lockfileVersion 3')
    locked_root = lock.get('packages', {}).get('', {})
    for section in ('dependencies', 'devDependencies', 'optionalDependencies', 'peerDependencies'):
        entries = manifest.get(section, {})
        if entries != locked_root.get(section, {}):
            errors.append(f'package.json: {section} do not agree with package-lock.json')
        for name, version in entries.items():
            locked = lock.get('packages', {}).get(f'node_modules/{name}', {})
            if isinstance(version, str) and PINNED_NPM_GIT.fullmatch(version):
                if locked.get('resolved') != version or not EXACT_VERSION.fullmatch(locked.get('version') or ''):
                    errors.append(f'package-lock.json: {name} must resolve the declared Git repository and full commit with an exact package version')
                continue
            release = SUITE_RELEASE.fullmatch(version) if isinstance(version, str) else None
            if release:
                # The lock records the tarball and its integrity; the version is the one of the release.
                if locked.get('resolved') != version or locked.get('version') != release.group(2) or not locked.get('integrity'):
                    errors.append(f'package-lock.json: {name} must resolve the release tarball it declares, with its integrity')
                continue
            if not isinstance(version, str) or not EXACT_VERSION.fullmatch(version):
                errors.append(f'package.json: {name} must use an exact version')
            if locked.get('version') != version:
                errors.append(f'package-lock.json: {name} resolved version differs from its declaration')
    mirror_verified, prepared_errors = validate_prepared_sidecar(root)
    errors.extend(prepared_errors)
    if prepared_errors:
        return errors
    for current, directories, files in os.walk(root, followlinks=False):
        # This exact mirror was already checked with a finite walk and per-file
        # fingerprints. No other source directory gains an exclusion.
        directories[:] = sorted(d for d in directories if d not in SKIP_DIRECTORIES and not (Path(current) / d).is_symlink() and not (mirror_verified and (Path(current)/d).relative_to(root) == SIDECAR_MIRROR))
        if 'package.json' in files and Path(current) != root:
            errors.append(f'Second npm manifest is forbidden: {(Path(current)/"package.json").relative_to(root).as_posix()}')
    return errors


def dependency_tables(manifest: dict):
    for section in DEPENDENCY_SECTIONS:
        yield section, manifest.get(section, {})
    for target, sections in manifest.get('target', {}).items():
        for section in DEPENDENCY_SECTIONS:
            yield f'target.{target}.{section}', sections.get(section, {})


def validate_cargo(root: Path, metadata: dict | None = None) -> list[str]:
    root = root.resolve()
    errors = []
    manifest = tomllib.loads((root/'Cargo.toml').read_text(encoding='utf-8'))
    allowed = manifest['workspace'].get('dependencies', {})
    for name, dependency in allowed.items():
        if isinstance(dependency, dict) and 'git' in dependency:
            # A release tag of the suite's own crate is allowed: Cargo.lock records the commit it points to, and imago-check holds the tag
            # equal to the npm release. Any other Git dependency needs a full immutable rev.
            tagged = name == 'imago-shell' and re.fullmatch(r'v\d+\.\d+\.\d+', dependency.get('tag', '')) and 'rev' not in dependency
            if not tagged and (not re.fullmatch(r'[0-9a-fA-F]{40}', dependency.get('rev', '')) or 'branch' in dependency or 'tag' in dependency):
                errors.append(f'Cargo.toml: Git dependency {name} needs a full immutable rev, without tag/branch')
            if 'branch' in dependency:
                errors.append(f'Cargo.toml: Git dependency {name} needs a full immutable rev, without tag/branch')
    manifests = []
    if 'package' in manifest:
        manifests.append(root/'Cargo.toml')
    for pattern in manifest['workspace'].get('members', []):
        for member in root.glob(pattern):
            manifests.append(member/'Cargo.toml' if member.is_dir() else member)
    manifests = {path.resolve() for path in manifests}
    if metadata is not None:
        packages = {package['id']:package for package in metadata['packages']}
        for identifier in metadata['workspace_members']:
            path = Path(packages[identifier]['manifest_path']).resolve()
            if not path.is_relative_to(root):
                errors.append('Cargo workspace member is outside the repository; declare and review its source explicitly')
                continue
            if path not in manifests:
                errors.append(f'Implicit Cargo workspace member must be listed in workspace.members: {path.relative_to(root).as_posix()}')
            # Cargo automatically enrolls in-tree path dependencies. Inspect those
            # manifests too, even while rejecting their implicit membership.
            manifests.add(path)
    for path in sorted(set(manifests)):
        data = tomllib.loads(path.read_text(encoding='utf-8'))
        for section, entries in dependency_tables(data):
            for name, dependency in entries.items():
                label = f'{path.relative_to(root).as_posix()} [{section}] {name}'
                if not isinstance(dependency, dict) or dependency.get('workspace') is not True:
                    errors.append(f'{label}: must inherit workspace = true')
                elif name not in allowed:
                    errors.append(f'{label}: missing from workspace.dependencies')
                elif set(dependency) - {'workspace', 'features', 'default-features', 'optional'}:
                    errors.append(f'{label}: member overrides dependency source/version')
    return errors


def cargo_roles(metadata: dict) -> dict[str, set[str]]:
    packages = {p['id']:p for p in metadata['packages']}
    nodes = {n['id']:n for n in metadata['resolve']['nodes']}
    members = set(metadata['workspace_members'])
    roles: dict[str, set[str]] = {}
    pending = [(member, 'runtime') for member in members]
    while pending:
        identifier, role = pending.pop()
        if role in roles.setdefault(identifier, set()):
            continue
        roles[identifier].add(role)
        for dependency in nodes[identifier]['deps']:
            for kind in dependency['dep_kinds']:
                if kind['kind'] == 'dev' and identifier not in members:
                    continue  # Registry crates' own tests are not compiled by this workspace.
                next_role = role
                if role == 'runtime':
                    if kind['kind'] == 'dev':
                        next_role = 'dev-test'
                    elif kind['kind'] == 'build' or any('proc-macro' in t['kind'] for t in packages[dependency['pkg']]['targets']):
                        next_role = 'build-tool'
                pending.append((dependency['pkg'], next_role))
    return roles


def supplements(root: Path) -> dict[tuple[str, str], dict]:
    result = {}
    text = (root/POLICY).read_text(encoding='utf-8')
    for match in re.finditer(r'<!-- supplement (\{[^\n]+\}) -->\s*```text\n(.*?)\n```', text, re.S):
        info = json.loads(match[1])
        body = match[2].replace('\r\n', '\n').strip() + '\n'
        if sha256(body) != info['sha256']:
            raise ValueError('Reviewed license supplement hash mismatch')
        for name, package in info['packages'].items():
            result[(name, package['version'])] = {**info, 'revision':package['revision'], 'text':body}
    return result


def license_files(base: Path, declared: str | None = None) -> list[dict]:
    candidates = [f for f in base.iterdir() if f.is_file() and f.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'COPYRIGHT', 'NOTICE'))]
    if declared:
        candidate = (base/declared).resolve()
        if candidate.is_relative_to(base.resolve()) and candidate.is_file():
            candidates.append(candidate)
    for directory in ('licenses', 'LICENSES'):
        if (base/directory).is_dir():
            candidates.extend(f for f in (base/directory).rglob('*') if f.is_file())
    result = []
    for path in sorted(set(candidates)):
        if path.is_symlink() or not path.resolve().is_relative_to(base.resolve()) or path.stat().st_size > 2*1024*1024:
            continue
        body = path.read_text(encoding='utf-8-sig', errors='strict').replace('\r\n', '\n').strip() + '\n'
        if body.strip():
            result.append({'file':path.relative_to(base).as_posix(), 'sha256':sha256(body), 'source':'packaged-file', 'text':body})
    return result


def has_license_text(texts: list[dict]) -> bool:
    return any(t['source'] in {'reviewed-supplement', 'reviewed-wrapper'} or t['file'].upper().startswith(('LICENSE', 'LICENCE', 'COPYING')) for t in texts)


def target_matches(values: list[str] | None, target: str) -> bool:
    if not values:
        return True
    positives = [value for value in values if not value.startswith('!')]
    return f'!{target}' not in values and (not positives or target in positives)


def npm_inventory(root: Path) -> tuple[list[dict], list[str]]:
    entries, errors = [], []
    lock = read_json(root/'package-lock.json')
    manifest = read_json(root/'package.json')
    direct = set().union(*(manifest.get(key, {}) for key in ('dependencies', 'devDependencies', 'optionalDependencies')))
    for key, package in sorted(lock['packages'].items()):
        if not key or not target_matches(package.get('os'), 'win32') or not target_matches(package.get('cpu'), 'x64'):
            continue
        name = key.rsplit('node_modules/', 1)[-1]
        version = package['version']
        base = root/key
        role = 'build-tool' if package.get('dev') else 'runtime'
        if not base.is_dir():
            errors.append(f'npm {name}@{version}: Windows package is not installed; run npm ci')
            local, texts = {}, []
        else:
            local = read_json(base/'package.json')
            if local.get('version') != version:
                errors.append(f'npm {name}: installed version does not match lockfile')
            texts = license_files(base)
            wrapper_name = NPM_LICENSE_WRAPPERS.get(name)
            if not has_license_text(texts) and wrapper_name:
                wrapper_base = root/'node_modules'/wrapper_name
                wrapper = read_json(wrapper_base/'package.json')
                locked_wrapper = lock['packages'].get(f'node_modules/{wrapper_name}', {})
                if (wrapper.get('version') == version == locked_wrapper.get('version')
                    and wrapper.get('optionalDependencies', {}).get(name) == version
                    and wrapper.get('license') == local.get('license')
                    and clean_url(wrapper.get('repository')) == clean_url(local.get('repository'))
                    and clean_url(wrapper.get('repository'))):
                    texts = [{**text, 'file':f"{wrapper_name}/{text['file']}", 'source':'reviewed-wrapper'} for text in license_files(wrapper_base)]
        license = package.get('license') or local.get('license')
        policy, notes = license_policy(license)
        if policy == 'error':
            errors.extend(f'npm {name}@{version}: {note}' for note in notes)
        if not has_license_text(texts):
            errors.append(f'npm {name}@{version}: missing packaged license text')
        entries.append({'ecosystem':'npm', 'name':name, 'version':version, 'license':license, 'roles':[role],
            'direct':name in direct and key == f'node_modules/{name}', 'policy':policy, 'review_notes':notes,
            'repository':clean_url(local.get('repository')), 'lock_integrity':package.get('integrity'), 'license_texts':texts})
    return entries, errors


def rust_inventory(root: Path, metadata: dict) -> tuple[list[dict], list[str]]:
    entries, errors = [], []
    roles = cargo_roles(metadata)
    members = set(metadata['workspace_members'])
    packages = {p['id']:p for p in metadata['packages']}
    nodes = {n['id']:n for n in metadata['resolve']['nodes']}
    direct = {dependency['pkg'] for member in members for dependency in nodes[member]['deps']}
    reviewed = supplements(root)
    locked = {(p['name'],p['version'],p.get('source')):p for p in tomllib.loads((root/'Cargo.lock').read_text(encoding='utf-8')).get('package', [])}
    for identifier in sorted(roles):
        if identifier in members:
            continue
        package = packages[identifier]
        base = Path(package['manifest_path']).parent
        texts = license_files(base, package.get('license_file'))
        supplement = reviewed.get((package['name'], package['version']))
        if not has_license_text(texts) and supplement:
            vcs = read_json(base/'.cargo_vcs_info.json')
            if vcs.get('git', {}).get('sha1') != supplement['revision'] or package.get('license') != supplement['license']:
                errors.append(f"Rust {package['name']}@{package['version']}: reviewed supplement provenance mismatch")
            else:
                texts.append({'file':'LICENSE (reviewed source supplement)', 'sha256':supplement['sha256'],
                    'source':'reviewed-supplement', 'url':supplement['source'], 'text':supplement['text']})
        policy, notes = license_policy(package.get('license'))
        if policy == 'error':
            errors.extend(f"Rust {package['name']}@{package['version']}: {note}" for note in notes)
        if not has_license_text(texts):
            errors.append(f"Rust {package['name']}@{package['version']}: missing packaged or reviewed license text")
        entries.append({'ecosystem':'Rust', 'name':package['name'], 'version':package['version'], 'license':package.get('license'),
            'roles':sorted(roles[identifier]), 'direct':identifier in direct, 'policy':policy, 'review_notes':notes,
            'repository':clean_url(package.get('repository')), 'lock_checksum':locked.get((package['name'],package['version'],package.get('source')), {}).get('checksum'), 'license_texts':texts})
    return entries, errors


def cell(value) -> str:
    return html.escape(str(value or '—')).replace('|', r'\|').replace('\n', ' ').replace('\r', ' ')


def markdown(entries: list[dict], assets: list[dict]) -> str:
    lines = ['# Third-party dependency notices', '',
        'Generated by `python scripts/dependencies.py generate` from the committed lockfiles and the Windows target graph.',
        'This inventory is not release legal approval and does not close X3. MPL-2.0 entries require release review of obligations and source availability.',
        'Runtime and build-tool reachability are conservative package-level classifications, not a binary link map. Proc macros are classified as build tools.',
        'Full packaged LICENSE/COPYRIGHT/NOTICE texts and reviewed supplements are generated in `.verification/THIRD_PARTY_LICENSES.txt`; distribute that file with prototype artifacts.',
        'System WebView2 and system fonts are external dependencies, not bundled assets. npm, Python and the Rust toolchain are build/test tools. The separately pinned sidecar source/runtime preparation is outside this Cargo/npm inventory. Its complete publisher Node license bundle is verified by sidecar/prepare-runtime.ps1; that check does not close X3 or certify a product release.', '',
        f'Target: `{TARGET}`. Indexed packages: {len(entries)}.', '',
        '| Ecosystem | Package | Version | Reachability | Declared license | Policy | License text files |',
        '| --- | --- | --- | --- | --- | --- | --- |']
    for entry in entries:
        texts = '; '.join(text['file'] for text in entry['license_texts'])
        lines.append('| ' + ' | '.join(cell(value) for value in [entry['ecosystem'], entry['name'], entry['version'], ', '.join(entry['roles']), entry['license'], entry['policy'], texts]) + ' |')
    lines += ['', '## Project-owned bundled assets', '', '| Asset | Origin | SHA-256 |', '| --- | --- | --- |']
    for asset in assets:
        lines.append('| ' + ' | '.join(cell(asset[key]) for key in ('path', 'origin', 'sha256')) + ' |')
    lines += ['', 'The icon source and generated variants are project-owned geometric artwork. No Oni-derived art, remote font or model is bundled. Prepared sidecar and Mukade sources are separately pinned third-party inputs, not project-owned assets or additional first-party npm packages; this table does not certify their transitive license completeness.', '',
        '## Review boundary', '',
        'Recognized SPDX identifiers are not a conclusion that every obligation has been discharged. Full texts do not replace a release review of source-disclosure, attribution, NOTICE and other applicable requirements.',
        'Missing or unknown declarations, unavailable required text, changed supplement provenance, manifest violations and stale notices fail the check. See `docs/implementation/dependency-policy.md` for reviewed missing-text sources.', '']
    return '\n'.join(lines)


def collect(root: Path, metadata: dict) -> tuple[list[dict], list[dict], list[str]]:
    entries, errors = rust_inventory(root, metadata)
    npm, npm_errors = npm_inventory(root)
    entries.extend(npm)
    errors.extend(npm_errors)
    entries.sort(key=lambda entry:(entry['ecosystem'], entry['name'].lower(), entry['version']))
    assets = []
    for path in sorted([root/'apps/desktop/app-icon.svg', *(root/'apps/desktop/src-tauri/icons').glob('*')]):
        if path.is_file():
            assets.append({'path':path.relative_to(root).as_posix(), 'origin':'project-owned original geometric icon', 'sha256':sha256(path.read_bytes())})
    return entries, assets, errors


def write_reports(root: Path, entries: list[dict], assets: list[dict], errors: list[str], notice: str):
    output = root/'.verification'
    output.mkdir(exist_ok=True)
    sanitized = [{**entry, 'license_texts':[{key:value for key,value in text.items() if key != 'text'} for text in entry['license_texts']]} for entry in entries]
    report = {'schema_version':1, 'target':TARGET, 'notice_sha256':sha256(notice), 'legal_release_approval':False,
        'lockfile_sha256':{name:sha256((root/name).read_bytes()) for name in ['Cargo.lock','package-lock.json'] if (root/name).is_file()},
        'summary':{'packages':len(entries), 'runtime':sum('runtime' in e['roles'] for e in entries),
            'build_or_test':sum(any(r != 'runtime' for r in e['roles']) for e in entries),
            'release_review_required':sum(e['policy'] == 'release-review-required' for e in entries), 'errors':len(errors)},
        'packages':sanitized, 'assets':assets, 'errors':errors}
    (output/'dependencies.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    texts = ['THIRD-PARTY LICENSE AND NOTICE TEXTS', f'Target: {TARGET}', 'Generated from locked package files and explicitly reviewed source supplements. X3 legal release review remains open.', '']
    for entry in entries:
        texts += ['='*78, f"{entry['ecosystem']} {entry['name']} {entry['version']} ({', '.join(entry['roles'])})", f"Declared license: {entry['license']}"]
        if entry.get('repository'):
            texts.append(f"Upstream repository: {entry['repository']}")
        for text in entry['license_texts']:
            texts += ['', f"--- {text['file']} | SHA-256 {text['sha256']} ---"]
            if text.get('url'):
                texts.append(f"Reviewed source: {text['url']}")
            texts.append(text['text'])
    (output/'THIRD_PARTY_LICENSES.txt').write_text('\n'.join(texts),encoding='utf-8')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['generate', 'check'])
    parser.add_argument('--offline', action='store_true', help='Use already restored locked Cargo metadata only')
    args = parser.parse_args()
    errors = validate_npm(ROOT) + validate_cargo(ROOT)
    if errors:
        write_reports(ROOT,[],[],errors,'Inventory not generated because manifest policy failed.\n')
        for error in errors:
            print(f'ERROR: {error}',file=sys.stderr)
        return 1
    command = ['cargo', 'metadata', '--format-version', '1', '--locked', '--filter-platform', TARGET, '--features', 'shinatama-desktop/custom-protocol']
    if args.offline:
        command.append('--offline')
    metadata_run = subprocess.run(command,cwd=ROOT,capture_output=True,text=True,encoding='utf-8')
    if metadata_run.returncode:
        print('ERROR: locked Cargo metadata unavailable; restore dependencies and rerun cargo metadata locally for detailed diagnostics.',file=sys.stderr)
        return 1
    metadata = json.loads(metadata_run.stdout)
    errors.extend(validate_cargo(ROOT,metadata))
    if errors:
        write_reports(ROOT,[],[],errors,'Inventory not generated because actual Cargo workspace membership failed policy.\n')
        for error in errors:
            print(f'ERROR: {error}',file=sys.stderr)
        return 1
    entries, assets, inventory_errors = collect(ROOT,metadata)
    errors.extend(inventory_errors)
    notice = markdown(entries,assets)
    if args.command == 'generate' and not errors:
        (ROOT/NOTICE).write_text(notice,encoding='utf-8')
    elif args.command == 'check' and (not (ROOT/NOTICE).is_file() or (ROOT/NOTICE).read_text(encoding='utf-8') != notice):
        errors.append(f'{NOTICE} is stale; run python scripts/dependencies.py generate and review its diff')
    write_reports(ROOT,entries,assets,errors,notice)
    for error in errors:
        print(f'ERROR: {error}',file=sys.stderr)
    review = sum(entry['policy'] == 'release-review-required' for entry in entries)
    print(f"{'FAIL' if errors else 'PASS'}: {len(entries)} Windows dependency packages; {review} require release review; {len(errors)} inventory errors. X3 remains open.")
    return bool(errors)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, TypeError) as error:
        # Do not leak local cache/user paths into shared logs or reports.
        print(f'ERROR: dependency inventory could not be completed ({type(error).__name__}); inspect local manifests/cache and reviewed supplements.',file=sys.stderr)
        raise SystemExit(1)
