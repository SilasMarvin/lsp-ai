# CI pipelines

## Release

To create a new release for `lsp-ai`, all you'll need to do is create a new branch with the following format: `release/{release_name}`. `release_name` is usually the version of the release package in SemVer format `x.x.x{-rcx}`.

This has the advantage of being able to fix issues for a specific release while continuing developping on the `main` branch by cherry-picking patches. It's inspired by trunk-based development.

