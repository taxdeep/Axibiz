"""Verify noninteractive release commands cannot open a protected test company.

Supply a disposable fixture created by the auth UI regression, not a live company.
No password is accepted as an argument or environment value.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixture', required=True)
    parser.add_argument('--program', default='dist/Axibiz Desktop Beta.exe')
    args = parser.parse_args()
    fixture = Path(args.fixture).resolve(strict=True)
    program = Path(args.program).resolve(strict=True)
    with sqlite3.connect(fixture.as_uri() + '?mode=ro', uri=True) as connection:
        assert connection.execute('SELECT enabled FROM company_login WHERE id=1').fetchone()[0] == 1
    before = hashlib.sha256(fixture.read_bytes()).hexdigest()
    result = subprocess.run(
        [str(program), '--db', str(fixture), 'accounting-check'],
        stdin=subprocess.DEVNULL, capture_output=True, text=True,
        encoding='utf-8', errors='replace', timeout=60,
    )
    assert result.returncode != 0, 'Protected command must require sign-in'
    assert 'requires interactive sign-in' in result.stderr, result.stderr
    assert hashlib.sha256(fixture.read_bytes()).hexdigest() == before, 'Unauthenticated command changed company data'
    summary = {'noninteractive_command_rejected': True, 'sign_in_required': True, 'fixture_unchanged': True, 'exit_code': result.returncode}
    fixture.with_suffix('.cli-validation.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
