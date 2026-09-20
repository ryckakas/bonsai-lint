# Shipping the CLI inside the extension

[← Back to the roadmap](../../ROADMAP.md)

**Status:** analysed. The cheap half is a release step, the rest is a later decision.

The extension has to find a `bonsai-lint` binary before it can report anything. Today it looks
in three places, in order: the `bonsai-lint.path` setting, an npm package bundled beside it,
then `PATH`. The question is how much of that binary should travel inside the `.vsix`.

## How the bundled route works

The npm package does not need the executable to be present when it is packaged. The wrapper
fetches it the first time it is asked to run:

```js
run(binaryName) {
  const promise = !this.exists() ? this.install(true) : Promise.resolve();
```

So bundling the wrapper alone is enough to make the extension self-sufficient. It costs a
network call on first analysis and nothing afterwards. This is what phpcognit ships.

## The options, measured

| Approach | `.vsix` size | Uploads per release | First run |
| --- | --- | --- | --- |
| No dependency (what 0.1.0 packages today) | 66 KB | 1 | fails unless a binary is on `PATH` |
| Wrapper only | 89 KB | 1 | downloads about 1 MB |
| One `.vsix` per platform | about 1.6 MB each | 5 | nothing to fetch |
| All five binaries in one `.vsix` | 7.93 MB | 1 | nothing to fetch |

Per platform, zipped as a `.vsix` stores them:

```text
aarch64-apple-darwin        1.52 MB      (6.34 MB uncompressed)
aarch64-unknown-linux-gnu   1.54 MB      (6.46 MB)
x86_64-apple-darwin         1.59 MB      (6.54 MB)
x86_64-unknown-linux-gnu    1.64 MB      (6.86 MB)
x86_64-pc-windows-msvc      1.64 MB      (6.91 MB)
```

Adding the wrapper brings in `detect-libc` and nothing else. No platform-specific file is
packaged, so a single `.vsix` still installs everywhere. That was the risk worth checking and
it does not materialise.

## What platform-specific publishing would cost

VS Code supports it directly. `vsce package --target darwin-arm64` and its four siblings
produce one package per platform and the marketplace serves each user the matching one, which
is how rust-analyzer and the C++ extension work. Three things stand between here and there.

**Five uploads per release.** Publishing is manual because `vsce` needs an Azure DevOps token,
so this multiplies the one step that is already done by hand.

**A script to collect the other platforms.** The npm package downloads only the binary for the
machine running `npm install`. Building a `win32-x64` package on a Mac means fetching that
target's artifact from the GitHub Release, putting it where the extension expects it, and
packaging with the matching `--target`. It is perhaps thirty lines, but it is new build
machinery that has to stay in step with whatever `dist` produces.

**A fourth step in `binary.ts`.** It would need to look for a binary shipped beside the
extension before falling back to the npm wrapper.

Putting all five in one package avoids the per-platform machinery but still needs the
collection script, and every user downloads 7.93 MB to use 1.6 MB of it.

## Recommendation

Take the wrapper for now. It is a one line dependency, it is the step the release runbook
already calls for once the npm package exists, and it turns a `.vsix` that cannot find a binary
into one that fetches its own. One upload, works everywhere, 23 KB larger.

The case for platform-specific packages is not size, it is the first-use download. A corporate
proxy that blocks GitHub Releases makes the extension look broken, and `bonsai-lint.path` is
the escape hatch for exactly that. When that starts generating questions, the extra 1.5 MB per
install is worth paying. By then the publish is likely automated anyway, and five targets costs
no more effort than one.
