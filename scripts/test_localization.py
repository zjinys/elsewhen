"""Negative contracts: untranslated additions and broken ICU must fail CI."""
from copy import deepcopy
from contextlib import redirect_stdout, redirect_stderr
import importlib.util
import io
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('localization', Path(__file__).with_name('check-localization.py'))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

class ResourceGuardTest(unittest.TestCase):
    def setUp(self):
        self.resources = [
            {'items': '{count} 项', '@items': {'placeholders': {'count': {'type': 'int'}}}},
            {'items': '{count, plural, one {One item} other {{count} items}}'},
        ]

    def valid(self, catalogs):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return checker.check_resources(catalogs)

    def test_numeric_plural_is_valid(self):
        self.assertTrue(self.valid(self.resources))

    def test_missing_key_and_empty_translation_fail(self):
        for mutation in ({}, {'items': ''}, {'different': 'value'}):
            self.assertFalse(self.valid([self.resources[0], mutation]))

    def test_placeholder_type_name_and_icu_failures(self):
        for value in (
            '{count, plural, one {One}}',
            '{count, plural, other {A} other {B}}',
            '{count, plural, invalid {A} other {B}}',
            '{count, plural, other {unterminated}',
            '{wrong} items',
        ):
            self.assertFalse(self.valid([self.resources[0], {'items': value}]), value)
        changed = deepcopy(self.resources)
        changed[0]['@items']['placeholders']['count']['type'] = 'String'
        self.assertFalse(self.valid(changed))

if __name__ == '__main__':
    unittest.main()
