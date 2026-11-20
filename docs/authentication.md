# Accounts, sessions and workspace access

Rill authenticates members on the host that stores their conversations. A fresh installation needs no email service, third-party identity provider or shared signing key. Operators serve the whole app through one HTTPS origin. [Deployment guide](deployment.md)

## Joining a team

A visitor can create an isolated workspace and becomes its owner. The owner can issue a reusable invitation code for their workspace, valid for 24 hours. A teammate enters that code, a display name, a unique login ID and a password. Both account creation and joining start a session. Invitation expiry is checked again inside the registration transaction.

Login IDs are case-insensitive and unique across the installation. Each account belongs to one workspace. Passwords contain 10–128 Unicode characters. Rill stores a separately salted Argon2id hash per account. The parameters in this version are 19 MiB of memory, two iterations and one lane. Verification runs outside the database lock, with at most four concurrent password jobs. Repeated incorrect passwords for a known account trigger a one-minute throttle after ten failures. An unknown login ID also goes through password verification with a dummy hash.

## Session lifetime and password changes

Rill generates an opaque 32-byte random secret for each session and invitation. The database stores a SHA-256 digest of each secret. The session cookie is host-only, HTTP-only, `SameSite=Strict` and valid for 30 days. Production mode adds `Secure`. Account and conversation APIs send `Cache-Control: no-store`. State-changing browser requests must carry JSON and pass same-origin checks.

| Action | Effect |
| --- | --- |
| Sign in or switch accounts in a browser | Replace the browser's previous session. Other independent sessions remain valid. |
| Sign out | Revoke that cookie server-side and close its live event streams. |
| Change password from the account menu | Verify the current password, save the new hash, revoke every session for that member and issue a fresh cookie to the browser making the change. This is one transaction. Other devices must sign in again. |
| Open the same origin in another tab | Tabs share the browser's cookie. Following a rotation they can reconnect with the replacement session. |
| Reach the session expiry time | Reject subsequent authenticated requests and end the open stream. |

Concurrent login and password-change requests recheck the password hash inside the write transaction. A login verified against an old password cannot create a session after that password has been replaced.

## Who can see or change a message

| Data or action | Scope |
| --- | --- |
| Workspace channels and member list | Members of that workspace |
| Direct-message history, search and live delivery | The two participants |
| Edit a message or a reply | Its author, with the revision the editor originally opened |
| Save a message and list saved items | The signed-in member, while they can access the original conversation |
| Issue an invitation | The workspace owner |

The service checks the session and conversation membership on the server. Private saves notify only that member's open sessions. Expiry, logout and password changes apply to event streams as well as HTTP reads. The host operator can read the database and its backups.

Rill currently requires the existing password to change it. Email recovery, multi-factor authentication and organization SSO are open design work. An invitation grants a new membership and does not recover an existing account. [API contract](../.kiro/specs/workspace-chat/protocol.md)
