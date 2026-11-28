# The sync-paths App

`sync-paths` (`.github/workflows/fork-sync-paths.yml`) is the required check that stops a PR from merging without a person when it changes protected paths (`docs/fork/weekly-sync.md` § Trust). While GitHub Actions reports it, a workflow added in a same-repo PR can report a passing job with the same name. Pinning the check to GitHub Actions (`integration_id` 15368) doesn't help, because the fake reports through Actions too (cameronsjo/herdr-app#62).

The fix is a GitHub App that only this repo installs. The workflow posts the rules' result as that App's `sync-paths` check run, and the ruleset requires `sync-paths` from that App. A PR's workflow can't post as the App, because the App's key lives in an environment that only `master` may deploy to.

## How it fits together

- The `sync-paths` job runs from `pull_request_target`, so it is always master's copy of the workflow, and it checks out nothing from the PR.
- The job uses the environment `sync-paths-app`. Its deployment branches are `master` only, so a `pull_request` workflow from a PR branch is refused the environment and never sees its secret.
- After the rules run, the step "Post the result as the sync-paths App" signs a JWT with the App's key, asks for an installation token scoped to this repo with `checks: write` alone, and posts a completed `sync-paths` check run on the PR's head commit. It runs only when the repo variable `SYNC_PATHS_APP_ID` is set.
- The Actions job keeps failing on its own when the rules fail, so the check means the same thing before the App is set up as after.

## Setting it up (Cameron, once)

1. **Create the App.** Go to GitHub → Settings → Developer settings → GitHub Apps → New GitHub App.
   - Name: `herdr-app sync-paths` (any unused name works).
   - Homepage URL: `https://github.com/cameronsjo/herdr-app`.
   - Webhook: untick Active.
   - Repository permissions: Checks, Read and write. Leave every other permission at No access.
   - Where can this App be installed: Only on this account.
   - Create it, and note the App ID shown at the top of its page.
2. **Make a private key.** On the App's page, choose Generate a private key, which downloads a `.pem` file.
3. **Install it** on `cameronsjo/herdr-app` only (Install App → Only select repositories).
4. **Create the environment.** In the repo, go to Settings → Environments → New environment and name it `sync-paths-app`. Under Deployment branches and tags, pick Selected branches and tags and add `master`. Add an environment secret `SYNC_PATHS_APP_KEY` holding the whole `.pem` file. Put this secret on the environment only, never as a repository secret: a repository secret is readable by any same-repo PR's workflow. Then delete the downloaded `.pem`.
5. **Turn it on.** Add a repository variable `SYNC_PATHS_APP_ID` set to the App ID (Settings → Secrets and variables → Actions → Variables).
6. **Check it.** Push any change to an open PR. The PR should now show two `sync-paths` checks, one from GitHub Actions and one from the App, with the same result.
7. **Require the App's check.** In ruleset 27338965, change the required `sync-paths` check's source from GitHub Actions (or any source) to the new App. Leave `test` and `browser-webkit` as they are.
8. **Stage a fake once.** Open a throwaway same-repo PR that adds a workflow whose job is named `sync-paths` and passes, on a `claude/*` branch that touches `scripts/fork/`. It must stay blocked: the App's check fails, and the fake Actions job doesn't count. Then close the PR and delete its branch.

## Rotating or removing it

- Rotate the key by generating a new one on the App's page, replacing `SYNC_PATHS_APP_KEY`, and deleting the old key on the App's page.
- Before removing the App, switch the ruleset's `sync-paths` source back to GitHub Actions; otherwise every PR waits forever on a check that no longer comes. Then unset `SYNC_PATHS_APP_ID`.
