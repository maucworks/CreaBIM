import hashlib
import json
import sqlite3
import tempfile
import unittest
from contextlib import closing
from pathlib import Path

import project


class ProjectTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / 'project #1.creabim'
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute('PRAGMA application_id = 1129464393')
            db.execute('PRAGMA user_version = 1')
            db.execute('CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
            db.execute('CREATE TABLE elements (id TEXT PRIMARY KEY, ord INTEGER NOT NULL, kind TEXT NOT NULL, name TEXT NOT NULL, data TEXT NOT NULL)')
            db.execute('INSERT INTO meta VALUES (?, ?)', ('project_name', 'Example'))
            for order, (element_id, kind, name) in enumerate([
                ('a', 'Wall', 'External wall'),
                ('b', 'Wall', 'Internal wall'),
                ('c', 'Slab', 'Ground floor'),
            ]):
                data = {'id': element_id, 'name': name, 'kind': {kind: {'length': 2.0}}}
                db.execute('INSERT INTO elements VALUES (?, ?, ?, ?, ?)',
                           (element_id, order, kind, name, json.dumps(data)))

    def tearDown(self):
        self.temp.cleanup()

    def test_inspect_and_read_leave_database_unchanged(self):
        before = hashlib.sha256(self.path.read_bytes()).digest()
        result = project.inspect_project(self.path)
        self.assertEqual(result['metadata']['project_name'], 'Example')
        self.assertEqual(result['element_count'], 3)
        element = project.get_element(self.path, 'a')
        self.assertEqual(element['kind']['Wall']['length'], 2.0)
        self.assertEqual(hashlib.sha256(self.path.read_bytes()).digest(), before)

    def test_list_filters_and_paginates(self):
        result = project.list_elements(self.path, kind='Wall', search='wall', offset=1, limit=1)
        self.assertEqual(result['total'], 2)
        self.assertEqual(result['offset'], 1)
        self.assertEqual(result['limit'], 1)
        self.assertEqual(result['elements'], [{'id': 'b', 'kind': 'Wall', 'name': 'Internal wall'}])

    def test_invalid_inputs_fail_cleanly(self):
        for args in ({'limit': 0}, {'limit': 201}, {'offset': -1}, {'offset': 2**63},
                     {'limit': True}, {'kind': 1}, {'search': 1}):
            with self.subTest(args=args), self.assertRaises(ValueError):
                project.list_elements(self.path, **args)
        with self.assertRaises(ValueError):
            project.get_element(self.path, 'missing')
        with self.assertRaises(FileNotFoundError):
            project.inspect_project(self.path.with_name('missing.creabim'))

    def test_rejects_non_project_and_unknown_version(self):
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute('PRAGMA application_id = 0')
        with self.assertRaises(ValueError):
            project.inspect_project(self.path)
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute('PRAGMA application_id = 1129464393')
            db.execute('PRAGMA user_version = 2')
        with self.assertRaises(ValueError):
            project.inspect_project(self.path)


if __name__ == '__main__':
    unittest.main()
