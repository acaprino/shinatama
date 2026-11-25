import json
from pathlib import Path
import tempfile
import unittest
import subprocess
import shutil
import os
import tomllib

import dependencies


class DependencyPolicyTests(unittest.TestCase):
    def setUp(self):
        self.fixture_parent = dependencies.ROOT/'.verification/dependency-test-fixtures'
        self.fixture_parent.mkdir(parents=True, exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(prefix='fixture-', dir=self.fixture_parent)
        self.root = Path(self.directory.name)
        self.addCleanup(self.cleanup)

    def cleanup(self):
        # Check the exact resolved fixture before TemporaryDirectory recurses.
        target = self.root.resolve()
        parent = self.fixture_parent.resolve()
        if target == parent or not target.is_relative_to(parent):
            raise RuntimeError('Refusing to clean a fixture outside its test directory')
        self.directory.cleanup()

    def write(self, name, value):
        path = self.root/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value) if isinstance(value, dict) else value, encoding='utf-8')

    def npm_fixture(self):
        self.write('package.json', {'dependencies':{'example':'1.2.3'}})
        self.write('package-lock.json', {'lockfileVersion':3,'packages':{
            '':{'dependencies':{'example':'1.2.3'}},
            'node_modules/example':{'version':'1.2.3','license':'MIT'},
        }})

    def test_exact_root_versions_and_lockfile_agreement(self):
        self.npm_fixture()
        self.assertEqual(dependencies.validate_npm(self.root), [])
        self.write('package.json', {'dependencies':{'example':'^1.2.3'}})
        errors = dependencies.validate_npm(self.root)
        self.assertTrue(any('exact version' in error for error in errors))
        self.assertTrue(any('do not agree' in error for error in errors))

    def test_git_packages_require_the_same_immutable_source_in_the_lock(self):
        source = 'git+https://github.com/example/widget.git#'
        pinned = source + 'a' * 40
        for declared, resolved, version, valid in [
            (pinned, pinned, '1.2.3', True),
            (pinned, source + 'b' * 40, '1.2.3', False),
            (pinned, pinned.replace('/example/', '/another/'), '1.2.3', False),
            (pinned, pinned, '', False),
            (source + 'develop', source + 'develop', '1.2.3', False),
        ]:
            with self.subTest(declared=declared, resolved=resolved, version=version):
                manifest = {'dependencies':{'widget':declared}}
                self.write('package.json', manifest)
                self.write('package-lock.json', {'lockfileVersion':3, 'packages':{
                    '':manifest, 'node_modules/widget':{'version':version, 'resolved':resolved},
                }})
                self.assertEqual(not dependencies.validate_npm(self.root), valid)

    def test_second_manifest_is_rejected_but_bench_and_dependencies_are_excluded(self):
        self.npm_fixture()
        self.write('node_modules/installed/package.json', {})
        self.write('verify/bench/package.json', {})
        self.assertEqual(dependencies.validate_npm(self.root), [])
        self.write('apps/desktop/package.json', {})
        self.assertTrue(any('Second npm manifest' in error for error in dependencies.validate_npm(self.root)))

    def test_desktop_and_native_shell_use_the_same_imago_release(self):
        root = dependencies.ROOT
        cargo = tomllib.loads((root/'Cargo.toml').read_text(encoding='utf-8'))
        shell = cargo['workspace']['dependencies']['imago-shell']
        frontend = dependencies.read_json(root/'package.json')['dependencies']['@acaprino/imago']
        release = dependencies.SUITE_RELEASE.fullmatch(frontend)
        self.assertIsNotNone(release)
        self.assertEqual(f"v{release.group(2)}", shell['tag'])

    def prepared_sidecar_fixture(self):
        self.npm_fixture()
        entries = []
        for name in sorted(dependencies.SIDECAR_MANIFESTS):
            relative = dependencies.SIDECAR_MIRROR/name
            self.write(relative, {'name':'synthetic-upstream', 'license':'MIT'})
            body = (self.root/relative).read_bytes()
            entries.append({'path':name, 'bytes':len(body), 'sha256':dependencies.sha256(body)})
        pin = {'formatVersion':1, 'packageName':'billion-context', 'sourceFiles':entries[:2], 'artifactFiles':entries[2:]}
        self.write(dependencies.SIDECAR_PIN, pin)
        return pin

    def test_prepared_sidecar_manifests_require_exact_paths_and_pinned_hashes(self):
        pin = self.prepared_sidecar_fixture()
        self.assertEqual(dependencies.validate_npm(self.root), [])
        manifest = dependencies.SIDECAR_MIRROR/'kernel/package.json'
        original = (self.root/manifest).read_bytes()
        (self.root/manifest).write_bytes(original.replace(b'MIT', b'ISC'))  # Same length, different SHA-256.
        self.assertTrue(any('provenance failed' in error for error in dependencies.validate_npm(self.root)))
        (self.root/manifest).write_bytes(original)
        unknown = 'dist/unreviewed/package.json'
        self.write(dependencies.SIDECAR_MIRROR/unknown, {})
        body = (self.root/dependencies.SIDECAR_MIRROR/unknown).read_bytes()
        pin['artifactFiles'].append({'path':unknown, 'bytes':len(body), 'sha256':dependencies.sha256(body)})
        self.write(dependencies.SIDECAR_PIN, pin)
        self.assertTrue(any('Second npm manifest' in error and unknown in error for error in dependencies.validate_npm(self.root)))

    def test_prepared_sidecar_pin_rejects_escape_and_unbounded_file_lists(self):
        pin = self.prepared_sidecar_fixture()
        pin['sourceFiles'][0]['path'] = '../package.json'
        self.write(dependencies.SIDECAR_PIN, pin)
        self.assertTrue(any('provenance failed' in error for error in dependencies.validate_npm(self.root)))
        pin = self.prepared_sidecar_fixture()
        pin['sourceFiles'] *= dependencies.SIDECAR_MAX_ENTRIES
        self.write(dependencies.SIDECAR_PIN, pin)
        self.assertTrue(any('provenance failed' in error for error in dependencies.validate_npm(self.root)))

    def test_prepared_sidecar_reparse_parent_cannot_gain_a_manifest_exception(self):
        self.prepared_sidecar_fixture()
        parent = self.root/dependencies.SIDECAR_MIRROR/'kernel'
        outside = self.root/'synthetic-outside'
        parent.rename(outside)
        try:
            os.symlink(outside, parent, target_is_directory=True)
        except OSError:
            self.skipTest('Creating a synthetic directory symlink is unavailable on this host')
        self.addCleanup(lambda: parent.unlink())
        self.assertTrue(any('provenance failed' in error for error in dependencies.validate_npm(self.root)))

    def test_cargo_target_and_build_dependencies_must_inherit(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["crate"]\n[workspace.dependencies]\nserde="1"\n')
        self.write('crate/Cargo.toml', '[package]\nname="test"\nversion="0.1.0"\n[dependencies]\nserde.workspace=true\n[target.\'cfg(windows)\'.build-dependencies]\nserde={version="1"}\n')
        errors = dependencies.validate_cargo(self.root)
        self.assertEqual(len(errors), 1)
        self.assertIn('build-dependencies', errors[0])

    def test_member_source_override_and_unpinned_git_are_rejected(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["crate"]\n[workspace.dependencies]\nexternal={git="https://example.test/repository",branch="main"}\n')
        self.write('crate/Cargo.toml', '[package]\nname="test"\nversion="0.1.0"\n[dependencies]\nexternal={workspace=true,version="1"}\n')
        errors = dependencies.validate_cargo(self.root)
        self.assertTrue(any('immutable rev' in error for error in errors))
        self.assertTrue(any('overrides' in error for error in errors))

    @unittest.skipUnless(shutil.which('cargo'), 'Cargo is required to prove implicit workspace enrollment')
    def test_actual_implicitly_enrolled_path_crate_is_rejected_and_inspected(self):
        self.write('Cargo.toml', '[workspace]\nmembers=["app"]\nresolver="3"\n[workspace.dependencies]\nhelper={path="helper"}\n')
        self.write('app/Cargo.toml', '[package]\nname="app"\nversion="0.1.0"\n[dependencies]\nhelper.workspace=true\n')
        self.write('helper/Cargo.toml', '[package]\nname="helper"\nversion="0.1.0"\n[dependencies]\nthirdparty={path="../thirdparty"}\n')
        self.write('thirdparty/Cargo.toml', '[package]\nname="thirdparty"\nversion="0.1.0"\n')
        for name in ['app', 'helper', 'thirdparty']:
            self.write(f'{name}/src/lib.rs', '')
        # The explicit-member preflight alone cannot see the helper's violation.
        self.assertEqual(dependencies.validate_cargo(self.root), [])
        result = subprocess.run(['cargo','metadata','--no-deps','--offline','--format-version','1','--manifest-path',str(self.root/'Cargo.toml')],
            cwd=self.root,capture_output=True,text=True,encoding='utf-8')
        self.assertEqual(result.returncode, 0, 'Synthetic local-only Cargo metadata fixture failed')
        metadata = json.loads(result.stdout)
        self.assertEqual(len(metadata['workspace_members']), 3)
        errors = dependencies.validate_cargo(self.root,metadata)
        self.assertTrue(any('Implicit Cargo workspace member' in error and 'helper/Cargo.toml' in error for error in errors))
        self.assertTrue(any('helper/Cargo.toml [dependencies] thirdparty: must inherit' in error for error in errors))

    def test_spdx_parser_rejects_unknown_ids_and_broken_expressions(self):
        for expression in [None, 'LicenseRef-company', 'MIT OR', 'MIT AND (Apache-2.0', 'MIT WITH unknown-exception', 'SEE LICENSE IN terms.txt']:
            self.assertEqual(dependencies.license_policy(expression)[0], 'error')
        self.assertEqual(dependencies.license_policy('MIT/Apache-2.0')[0], 'recognized')
        self.assertEqual(dependencies.license_policy('(MIT OR Apache-2.0) AND Unicode-3.0')[0], 'recognized')
        self.assertEqual(dependencies.license_policy('MPL-2.0')[0], 'release-review-required')

    def test_runtime_and_build_classification_does_not_ship_proc_macros(self):
        def package(identifier, macro=False):
            return {'id':identifier,'targets':[{'kind':['proc-macro' if macro else 'lib']}]}
        def edge(identifier, kind=None):
            return {'pkg':identifier,'dep_kinds':[{'kind':kind}]}
        metadata = {'workspace_members':['app'], 'packages':[package('app'),package('runtime'),package('macro',True),package('tool'),package('test')],
            'resolve':{'nodes':[
                {'id':'app','deps':[edge('runtime'),edge('macro'),edge('tool','build'),edge('test','dev')]},
                {'id':'runtime','deps':[]},{'id':'macro','deps':[edge('runtime')]},{'id':'tool','deps':[]},{'id':'test','deps':[]}]}}
        roles = dependencies.cargo_roles(metadata)
        self.assertEqual(roles['macro'], {'build-tool'})
        self.assertEqual(roles['runtime'], {'runtime','build-tool'})
        self.assertEqual(roles['test'], {'dev-test'})

    def test_npm_lock_classifies_runtime_and_dev_and_omits_foreign_bindings(self):
        self.write('package.json', {'dependencies':{'live':'1.0.0'},'devDependencies':{'tool':'2.0.0'}})
        self.write('package-lock.json', {'packages':{'':{},
            'node_modules/live':{'version':'1.0.0','license':'MIT'},
            'node_modules/tool':{'version':'2.0.0','license':'MIT','dev':True},
            'node_modules/mac-only':{'version':'1.0.0','license':'MIT','dev':True,'os':['darwin']}}})
        for name, version in [('live','1.0.0'),('tool','2.0.0')]:
            self.write(f'node_modules/{name}/package.json', {'version':version})
            self.write(f'node_modules/{name}/LICENSE', 'MIT license fixture')
        entries, errors = dependencies.npm_inventory(self.root)
        self.assertEqual(errors, [])
        self.assertEqual([(e['name'],e['roles']) for e in entries], [('live',['runtime']),('tool',['build-tool'])])

    def test_missing_license_text_fails_even_with_known_license_identifier(self):
        self.npm_fixture()
        self.write('node_modules/example/package.json', {'version':'1.2.3'})
        entries, errors = dependencies.npm_inventory(self.root)
        self.assertEqual(entries[0]['policy'], 'recognized')
        self.assertTrue(any('missing packaged license text' in error for error in errors))

    def test_wrapper_text_requires_exact_version_relationship_and_same_provenance(self):
        child='@rolldown/binding-win32-x64-msvc'
        self.write('package.json', {'devDependencies':{'rolldown':'1.2.3'}})
        self.write('package-lock.json', {'packages':{'':{},f'node_modules/{child}':{'version':'1.2.3','license':'MIT','dev':True},'node_modules/rolldown':{'version':'1.2.3','license':'MIT','dev':True}}})
        common={'version':'1.2.3','license':'MIT','repository':'https://example.test/repository'}
        self.write(f'node_modules/{child}/package.json', common)
        self.write('node_modules/rolldown/package.json', {**common,'optionalDependencies':{child:'1.2.3'}})
        self.write('node_modules/rolldown/LICENSE', 'MIT license fixture')
        entries, errors = dependencies.npm_inventory(self.root)
        self.assertEqual(errors, [])
        self.assertEqual(entries[0]['license_texts'][0]['source'], 'reviewed-wrapper')
        self.write('node_modules/rolldown/package.json', {**common,'optionalDependencies':{child:'2.0.0'}})
        self.assertTrue(dependencies.npm_inventory(self.root)[1])

    def test_notice_render_is_deterministic_and_escapes_table_content(self):
        entry={'ecosystem':'npm','name':'bad|<script>\nname','version':'1.0.0','roles':['runtime'],'license':'MIT','policy':'recognized','license_texts':[{'file':'LICENSE'}]}
        first=dependencies.markdown([entry],[])
        self.assertEqual(first,dependencies.markdown([entry],[]))
        self.assertIn(r'bad\|&lt;script&gt; name',first)
        self.assertNotIn('<script>',first)

    def test_sanitized_reports_do_not_contain_cache_paths_or_full_text(self):
        text={'file':'LICENSE','source':'packaged-file','sha256':'abc','text':'license fixture body'}
        entry={'ecosystem':'Rust','name':'sample','version':'1.0.0','roles':['runtime'],'license':'MIT','policy':'recognized','license_texts':[text]}
        dependencies.write_reports(self.root,[entry],[],[],'notice')
        report=(self.root/'.verification/dependencies.json').read_text()
        self.assertNotIn(str(self.root),report)
        self.assertNotIn('license fixture body',report)
        self.assertIn('license fixture body',(self.root/'.verification/THIRD_PARTY_LICENSES.txt').read_text())

    def test_url_sanitization_drops_credentials_query_and_local_urls(self):
        self.assertEqual(dependencies.clean_url('git+https://secret:token@example.test/repo?token=hidden#private'),'https://example.test/repo')
        self.assertIsNone(dependencies.clean_url('file:///private/cache/package'))


if __name__ == '__main__':
    unittest.main()
