"""Verify a release against a disposable online backup, never the live company.

Run from the repository root: python scripts/verify_upgrade.py
The copied company's identity is changed solely to avoid running-instance conflicts.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import time
import uuid


def quote(identifier):
    return '"' + identifier.replace('"', '""') + '"'


def rows(connection, table, columns):
    sql = 'SELECT ' + ','.join(map(quote, columns)) + ' FROM ' + quote(table)
    return sorted(connection.execute(sql).fetchall(), key=repr)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', default='database/tasks.db')
    parser.add_argument('--program', default='dist/Axibiz Desktop Beta.exe')
    args = parser.parse_args()
    source = Path(args.source).resolve(strict=True)
    program = Path(args.program).resolve(strict=True)
    isolation = uuid.uuid4().hex
    root = Path('target').resolve() / ('verify-upgrade-' + str(time.time_ns()) + '-' + isolation[:8])
    root.mkdir(parents=True, exist_ok=False)
    if os.name == 'nt':
        # Apply the application's own ACL policy before copying company data.
        policy = (Path(__file__).resolve().parents[1] / 'src/security.rs').read_text(encoding='utf-8')
        script = re.search(r'const SCRIPT: &str = r#"(.*?)"#;', policy, re.S).group(1)
        environment = dict(os.environ, AXIBIZ_SECURITY_PATH=str(root), AXIBIZ_SECURITY_TREE='1')
        shell = Path(os.environ['SystemRoot']) / 'System32/WindowsPowerShell/v1.0/powershell.exe'
        subprocess.run([str(shell), '-NoProfile', '-NonInteractive', '-Command', script], env=environment, check=True, capture_output=True, creationflags=subprocess.CREATE_NO_WINDOW)
    destination = root / 'company.db'
    before_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    live = sqlite3.connect(source.as_uri() + '?mode=ro', uri=True)
    clone = sqlite3.connect(destination)
    try:
        live.backup(clone)
    finally:
        live.close()
    old_version = clone.execute('PRAGMA user_version').fetchone()[0]
    # Only the disposable copy is changed, before capturing its baseline.
    clone.create_function('axibiz_actor', 0, lambda: 'local')
    clone.execute('DROP TRIGGER IF EXISTS company_uuid_fixed')
    clone.execute('UPDATE company_identity SET uuid=? WHERE id=1', (str(uuid.uuid4()),))
    clone.execute("UPDATE settings SET company_name=company_name || ? WHERE id=1", (' [isolated verification ' + isolation + ']',))
    clone.commit()
    baseline = {}
    for (table,) in clone.execute("SELECT name FROM sqlite_master WHERE type='table'"):
        columns = [row[1] for row in clone.execute('PRAGMA table_info(' + quote(table) + ')')]
        baseline[table] = (columns, rows(clone, table, columns))
    clone.close()
    result = subprocess.run([str(program), '--db', str(destination), 'accounting-check'], capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=120)
    if result.returncode:
        raise RuntimeError('Release verification command failed: ' + result.stdout + result.stderr)
    checked = sqlite3.connect(destination.as_uri() + '?mode=ro', uri=True)
    assert checked.execute('PRAGMA integrity_check').fetchone()[0] == 'ok'
    assert not checked.execute('PRAGMA foreign_key_check').fetchall()
    for table, (columns, original_rows) in baseline.items():
        current = rows(checked, table, columns)
        if table == 'audit_log':
            assert all(row in current for row in original_rows), 'Original audit history changed'
        elif table == 'number_sequences' and old_version < 18 and not any(row[columns.index('kind')] == 'vendor_payment' for row in original_rows):
            kind_index = columns.index('kind')
            assert [row for row in current if row[kind_index] != 'vendor_payment'] == original_rows, 'Existing numbering sequences changed'
            added = [row for row in current if row[kind_index] == 'vendor_payment']
            assert len(added) == 1, 'Expected one new supplier-payment numbering sequence'
            values = dict(zip(columns, added[0]))
            assert values['prefix'] == 'VP' and values['width'] == 4 and values['next_value'] == 1
        else:
            assert current == original_rows, 'Original columns changed in ' + table
    new_version = checked.execute('PRAGMA user_version').fetchone()[0]
    checked.close()
    assert hashlib.sha256(source.read_bytes()).hexdigest() == before_hash, 'Source changed during verification'
    summary = {
        'from_schema': old_version, 'to_schema': new_version,
        'original_tables_preserved': len(baseline), 'source_file_unchanged': True,
        'integrity': 'ok', 'foreign_keys': 'ok',
        'review_warnings': sum(line.startswith('REVIEW ') for line in result.stdout.splitlines()),
        'copy': str(destination),
    }
    (root / 'result.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == '__main__':
    main()
