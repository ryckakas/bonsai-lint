# Security policy

## Supported versions

Only the latest release gets security fixes. bonsai-lint is pre-1.0, so a fix ships as a new
release rather than a backport; upgrade to pick it up.

## Reporting a vulnerability

Report it privately through GitHub:
**[Report a vulnerability](https://github.com/ryckakas/bonsai-lint/security/advisories/new)**.
Please don't open a public issue, pull request or discussion for it.

A useful report says which version and install source you used (npm, PyPI, Composer, Homebrew,
Cargo, the installer script or the editor extension), what you ran, and the input that triggers
it. A minimal source file that reproduces the problem is the most helpful thing you can attach.

The report stays private while it is investigated and fixed. Once a fixed release is out, the
advisory is published with credit to you, unless you'd rather not be named.

## What counts

bonsai-lint promises that scanning untrusted source is safe: it parses syntax and never executes,
imports or resolves the code it reads. Anything that breaks that promise is in scope, for example:

- Scanned source, or a config file, causing code execution, or a read or write outside what the
  run was pointed at.
- A crafted input that crashes the process or hangs it indefinitely, where that could take down a
  CI job or editor that scans code it doesn't control.
- A problem in a release artifact or the way it is built and published: the binaries, the npm,
  PyPI and Composer packages, the Homebrew formula, the installer script or the VS Code extension.

A vulnerability in a dependency that bonsai-lint doesn't actually reach is better raised as a
normal issue, or left to Dependabot, which already watches the dependencies.
