# Security policy

## Supported versions

Security fixes land on the latest released `0.x` version.

## Reporting a vulnerability

Please **do not** open a public issue for a vulnerability.

Use GitHub's **private vulnerability reporting** on this repository
(Security → Advisories → New draft security advisory). That channel
survives a repository transfer.

Include:

- A description of the issue
- Steps to reproduce
- The impact you expect
- Any suggested fix

You should hear back within a week. If the report is confirmed, we will
coordinate a fix and a credit (if you want one) in the CHANGELOG.

If a published crate version ever contains a secret, **rotate that
secret immediately**. `cargo yank` does not delete uploaded files.
