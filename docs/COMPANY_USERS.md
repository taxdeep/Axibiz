# Company Users and Optional Sign-In

Open **Setup > Users...**. Accounts, password verifiers and the sign-in policy belong to the company database, not the computer's Software Settings. Different companies may use the same usernames with independent passwords. A session is bound to the company's immutable identity and current user/policy revisions.

## First-time setup

Existing and new companies default to direct access: sign-in is off, with one active `admin` / `Administrator` account and no password. No new login prompt is imposed on an existing company.

To enable sign-in:

1. Open Setup > Users and select the administrator. You may change its username and display name.
2. Leave Current Password empty if none has been set. Enter and confirm a new password of 12–128 characters.
3. Select **Require password sign-in for this company** and click **Save User**.

Every active account must have a password before sign-in can be enabled. Otherwise the change is rejected and the draft remains available to correct. Turning sign-in off requires the administrator's current password; saved passwords can be retained for later re-enablement. With sign-in off, the workspace opens directly with local administrator access. Password storage alone does not enable the login requirement.

## Users and sessions

- Administrators can add accounts, change usernames/display names, reset passwords, change the company login policy and deactivate accounts. An active administrator must always remain. Deactivation preserves user IDs and historical audit identity; accounts are not deleted.
- A User can change their own display name/password using their current password. They cannot promote themselves, change another account or disable sign-in. Company restore also requires administrator access while sign-in is enabled. Business features otherwise remain shared; these roles are not a financial approval/segregation-of-duties system.
- **File > Lock / Switch User** is available when sign-in is enabled and all business/settings drafts are saved or discarded. The signed-out workspace renders only the sign-in screen; business menus and native business views do not render until authentication succeeds.
- User drafts survive navigation. Explicit Revert discards them; exit/restore checks include user/password drafts. Other open business/settings drafts must be saved/closed before changing accounts. Password inputs are masked and owned password buffers are cleared on failure/success/discard/drop as applicable; they are not serialized into UI preference files.
- No master/recovery password is built in. Keep administrator credentials, another active administrator where appropriate, and verified backups. An administrator can reset another user's password using their own credentials.

## Company backups and startup

Schema **21** adds `company_users` and `company_login`. Upgrade seeds the optional administrator without changing prior business columns or fabricating historical user attribution. Company backups include account/password/login state. Restoring one restores that snapshot's user policy, including whether sign-in was enabled; existing sessions are cleared and the restored company must be signed into using its own credentials. Older backups restore their older security state too.

Protected companies are authenticated before ordinary startup backup, schema migration and desktop creation. An existing protected legacy `tasks.db` is opened in its original location, allowing authentication before any legacy file relocation/copy. Unsupported future schemas and incomplete user configurations fail closed without creating a replacement passwordless administrator.

All CLI commands use the same startup gate and a terminal password prompt without echo. Protected companies reject noninteractive execution rather than accepting passwords in arguments/environment variables or bypassing login. `--help` and `--version` remain available. The internal command dispatcher also checks the company session. Backup validation may inspect an isolated in-memory copy without unlocking a workspace; it does not reset source credentials.

## Passwords and audit

Password verifiers use RustCrypto Argon2id v19 with a fresh OS-random salt, 19 MiB memory, two passes and one lane. Stored PHC parameters are bounded to the supported policy before verification. Passwords are not stored in plain text, recoverable encryption, preference JSON, invoice snapshots or audit JSON. Five failed attempts trigger a 30-second company-wide wait that survives restart; a successful attempt clears the counter. Administrative current-password confirmation uses the same throttling.

The parameters follow the [OWASP password-storage guidance](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html); the implementation uses the [RustCrypto Argon2 password-hashing API](https://docs.rs/argon2/0.5.3/argon2/). Credential updates and their metadata-only user audit event commit atomically. Failed login records contain neither password/hash nor attacker-supplied password text. Normal row-change audit triggers record the signed-in user ID/username; system/restore events can remain `local`. Connection bootstrap registers the audit-actor function; independent application connections must do the same.

This is application access control. SQLite and backup files remain unencrypted. Existing OS file access protection remains necessary; this feature does not prevent an OS-authorized person from reading or modifying raw files with another tool.

## Validation

Regressions cover independent companies with identical usernames, optional enable/disable, password changes, salt differences, standard-user escalation/restore refusal, last-administrator protection, passwordless-active-account rejection, forced user-audit rollback, persisted throttling, absence of secrets in audit JSON, restore/session invalidation, unsupported/corrupt authentication state, guarded CLI dispatch and protected legacy-path handling. Actual login/user widgets are captured at 1440×900 and 900×650; signed-out captures assert that the business navigation was not rendered. `scripts/verify_login.py` additionally exercises the release executable against a disposable protected fixture and checks that noninteractive access changes no database bytes.
