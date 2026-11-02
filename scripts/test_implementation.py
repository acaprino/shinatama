import unittest
import copy

import implementation


class CheckpointValidation(unittest.TestCase):
    def setUp(self):
        self.plan, self.progress = implementation.load()

    def test_current_plan_and_checkpoint_are_consistent(self):
        self.assertEqual(implementation.validate(self.plan, self.progress)[0], [])

    def test_missing_requirement_assignment_is_rejected(self):
        requirement = next(iter(implementation.requirements()))
        for step in self.plan['steps']:
            step['requirements'] = [item for item in step['requirements'] if item != requirement]
        self.assertTrue(any('Unplanned requirement' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_dependency_cycle_is_rejected(self):
        self.plan['steps'][0]['depends_on'] = [self.plan['steps'][-1]['id']]
        self.plan['steps'][-1]['depends_on'] = [self.plan['steps'][0]['id']]
        self.assertTrue(any('cycle' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_unknown_dependency_is_rejected(self):
        self.plan['steps'][0]['depends_on'] = ['nonexistent']
        self.assertTrue(any('unknown dependency' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_two_active_steps_are_rejected(self):
        for step in self.plan['steps'][:2]:
            self.progress['steps'][step['id']] = {'status': 'in_progress'}
        self.assertTrue(any('one active' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_commit_requires_real_sha_field(self):
        self.progress['steps']['B00']['status'] = 'committed'
        self.progress['steps']['B00']['commit_sha'] = None
        self.assertTrue(any('real full SHA' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_nonexistent_commit_is_rejected(self):
        self.progress['steps']['B00'].update(status='committed', commit_sha='0' * 40)
        self.assertTrue(any('existing Git commit' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_active_step_cannot_skip_pending_dependencies(self):
        self.progress['steps']['B01']['status'] = 'pending'
        self.progress['steps']['C02'] = {'status': 'in_progress'}
        self.assertTrue(any('dependency' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_active_post_v1_step_requires_explicit_opt_in(self):
        self.progress['steps']['B01']['status'] = 'pending'
        self.progress['steps']['C39'] = {'status': 'in_progress'}
        self.assertTrue(any('non-V1' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_large_increment_needs_commit_units(self):
        step = next(s for s in self.plan['steps'] if s.get('commit_units'))
        del step['commit_units']
        step['hours'] = [20, 30]
        self.assertTrue(any('large increment' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_parent_cannot_complete_before_its_commit_units(self):
        step = next(s for s in self.plan['steps'] if s.get('commit_units'))
        self.progress['steps'][step['id']] = copy.deepcopy(self.progress['steps']['B00'])
        self.assertTrue(any('every commit unit' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_commit_units_cannot_start_out_of_order(self):
        step = next(s for s in self.plan['steps'] if len(s.get('commit_units', [])) > 1)
        self.progress['steps']['B01']['status'] = 'pending'
        self.progress['steps'][step['id']] = {'status': 'in_progress', 'units': {step['commit_units'][1]['id']: {'status': 'in_progress'}}}
        self.assertTrue(any('completed previous units' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_next_step_waits_for_dependencies(self):
        plan = {'steps': [
            {'id': 'first', 'scope': 'v1', 'depends_on': []},
            {'id': 'second', 'scope': 'v1', 'depends_on': ['first']},
        ]}
        progress = {'steps': {}}
        self.assertEqual(implementation.next_step(plan, progress)['id'], 'first')
        progress['steps']['first'] = {'status': 'verified'}
        self.assertEqual(implementation.next_step(plan, progress)['id'], 'second')

    def test_active_scope_cannot_depend_on_deferred_work(self):
        step = next(s for s in self.plan['steps'] if s['id'] == 'C15')
        step['depends_on'].append('C34')
        self.assertTrue(any('active scope cannot depend on deferred' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_daodan_cannot_resume_through_non_v1_opt_in(self):
        self.progress['allow_non_v1'] = ['C34']
        self.progress['steps']['C34']['status'] = 'in_progress'
        self.assertTrue(any('new user instruction' in error for error in implementation.validate(self.plan, self.progress)[0]))

    def test_automatic_selection_skips_deferred_even_if_marked_active(self):
        plan = {'steps': [
            {'id': 'daodan', 'scope': 'deferred', 'depends_on': []},
            {'id': 'independent', 'scope': 'v1', 'depends_on': []},
        ]}
        progress = {'steps': {'daodan': {'status': 'in_progress'}}, 'allow_non_v1': ['daodan']}
        self.assertEqual(implementation.next_step(plan, progress)['id'], 'independent')


if __name__ == '__main__':
    unittest.main()
