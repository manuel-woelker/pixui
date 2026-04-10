# What does the current release process ship?

`pixui` currently ships:

- crates.io packages for `pixui-base` and `pixui-pal`
- a GitHub release containing packaged `.crate` artifacts for those crates

# How are release versions managed?

All workspace crates currently share one version.

The release script expects:

- `crates/base`
- `crates/pal`

to use the same version, and it verifies that all internal path dependencies are pinned to that same version.

# How do I prepare a new release version?

To update all workspace crate versions without publishing yet, run:

```bash
./scripts/release.sh prepare 0.1.1
```

That updates the crate manifests in place. Review the changes, commit them, and keep the worktree clean before publishing.

# How do I publish a release?

To publish the current shared version to crates.io and create a Git tag, run:

```bash
./scripts/release.sh publish
```

To prepare, commit, publish, and tag a new version in one step, run:

```bash
./scripts/release.sh release 0.1.1
```

# What checks run before publishing?

The release script verifies:

- the current branch is `main` unless overridden with `PIXUI_RELEASE_BRANCH`
- the git worktree is clean
- all workspace crate versions match
- all internal dependency versions match the release version
- the target version is not already published on crates.io
- `nao check` succeeds

It also requires crates.io credentials through `CARGO_REGISTRY_TOKEN` or an existing Cargo login.

# Is there a dry-run mode?

Yes. Use:

```bash
./scripts/release.sh publish --dry-run
```

or:

```bash
./scripts/release.sh --dry-run
```

Dry runs execute validation and packaging checks without publishing crates, creating tags, or pushing anything.

# What does the GitHub release workflow do?

When a tag matching `v*` is pushed, `.github/workflows/release.yml`:

- runs `cargo package` for `pixui-base` and `pixui-pal`
- attaches the generated `.crate` files to a GitHub release
