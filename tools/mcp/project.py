"""Read saved CreaBIM projects without modifying them."""

import json
import sqlite3
from pathlib import Path


def _open_project(path):
    path = Path(path).expanduser().resolve(strict=True)
    if not path.is_file() or path.suffix.lower() != '.creabim':
        raise ValueError('Expected an existing .creabim project file')
    db = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True)
    try:
        db.execute('BEGIN')
        app_id = db.execute('PRAGMA application_id').fetchone()[0]
        version = db.execute('PRAGMA user_version').fetchone()[0]
        if app_id != 0x43524249:
            raise ValueError('Not a CreaBIM project')
        if version != 1:
            raise ValueError(f'Unsupported CreaBIM project format version: {version}')
        return db
    except Exception:
        db.close()
        raise


def inspect_project(path):
    db = _open_project(path)
    try:
        metadata = dict(db.execute('SELECT key, value FROM meta'))
        count = db.execute('SELECT COUNT(*) FROM elements').fetchone()[0]
        return {'metadata': metadata, 'element_count': count}
    finally:
        db.close()


def list_elements(path, kind=None, offset=0, limit=50, search=None):
    if kind is not None and not isinstance(kind, str):
        raise ValueError('kind must be a string')
    if search is not None and not isinstance(search, str):
        raise ValueError('search must be a string')
    if type(offset) is not int or not 0 <= offset < 2**63:
        raise ValueError('offset must be an integer between 0 and 2^63 - 1')
    if type(limit) is not int or not 1 <= limit <= 200:
        raise ValueError('limit must be an integer between 1 and 200')

    filters, args = [], []
    if kind is not None:
        filters.append('kind = ?')
        args.append(kind)
    if search is not None:
        filters.append('instr(lower(name), lower(?)) > 0')
        args.append(search)
    where = ' WHERE ' + ' AND '.join(filters) if filters else ''

    db = _open_project(path)
    try:
        total = db.execute('SELECT COUNT(*) FROM elements' + where, args).fetchone()[0]
        rows = db.execute(
            'SELECT id, kind, name FROM elements' + where + ' ORDER BY ord LIMIT ? OFFSET ?',
            [*args, limit, offset],
        )
        elements = [dict(zip(('id', 'kind', 'name'), row)) for row in rows]
        return {'elements': elements, 'total': total, 'offset': offset, 'limit': limit}
    finally:
        db.close()


def get_element(path, element_id):
    if not isinstance(element_id, str) or not element_id:
        raise ValueError('element_id must be a non-empty string')
    db = _open_project(path)
    try:
        row = db.execute('SELECT data FROM elements WHERE id = ?', (element_id,)).fetchone()
        if row is None:
            raise ValueError(f'Element not found: {element_id}')
        return json.loads(row[0])
    finally:
        db.close()
