# Upstream mirror

`upstream/main` tracks `main` in https://github.com/samwhosung/benilla.
Keep fork-specific changes on `master` or feature branches, not on this mirror.
The sync never merges upstream into `master`.

The **Sync upstream main** workflow checks hourly at minute 17 UTC and can also
be run manually from the default branch. It becomes active only after its pull
request is merged into the default branch and Actions is enabled for this fork.
GitHub schedules are best-effort and may be delayed or dropped. Public-repository
schedules can be disabled after 60 days without repository activity; re-enable
this workflow in Actions if necessary. This is an hourly mirror, not an instant
upstream webhook.

Normal updates are fast-forwards. If upstream rewrites history, or somebody adds
local commits to the mirror, the workflow atomically saves the previous tip as
`upstream-archive/main-<full-old-SHA>` and replaces the mirror with the upstream
tip. The run summary and warning identify the archive. A concurrent branch change
causes the force-with-lease push to fail rather than overwriting unseen commits.
If archiving or updating is blocked by repository rules, the atomic push fails
without changing either ref. No archive branches are automatically removed.

Only the sync job receives the built-in `GITHUB_TOKEN` with `contents: write`.
It fetches Git objects without checking out or executing upstream code, and does
not require a personal token, new credential, or external service. Runner time is
subject to the repository's Actions billing settings. This separate schedule does
not start **Build player**, which remains manual-only.

After merging, inspect **Actions → Sync upstream main** for failures and compare
`upstream/main` with upstream `main`. To test immediately, manually run the sync
workflow from the default branch. Branch rules may need owner review if they
reject the mirror/archive updates; the workflow does not bypass them.
