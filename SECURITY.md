# Security policy

## Reporting a vulnerability

Please report security issues privately through GitHub's
[private vulnerability reporting](https://github.com/ganddtn40/soundy/security/advisories/new)
instead of opening a public issue.

Include the version, the steps to reproduce, and the impact you believe the issue has. You can
expect an acknowledgement within a few days. Please give the project time to ship a fix before
disclosing the issue publicly.

## Scope

Soundy is a local desktop application. Relevant reports include:

- Memory-safety or logic bugs that let a crafted file, playlist, link or provider response run
  code or read unexpected data.
- Credential or token handling bugs in provider sign-in or local storage.
- Cases where the app sends data somewhere the privacy policy does not describe.
- Installer or update-path issues that could replace or tamper with the binary.

Issues in third-party services, provider APIs or the Sonora codebase itself should also be
reported upstream where appropriate. Soundy has no bug bounty programme.
