# Windows code signing (SignPath)

RxTerm's Windows installers are signed via [SignPath Foundation](https://signpath.org/),
which provides free code signing for open-source projects. The release
workflow (`.github/workflows/release.yml`) has a `sign-windows` job that is
**inert until you complete the one-time setup below** and set the
`SIGNING_ENABLED` repository variable to `true`. Until then, releases ship
unsigned and the job is skipped.

## What signing does (and does not) fix

Two separate Windows warnings, per
[Microsoft's current docs](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation):

- **UAC "Unknown Publisher"** — fixed **immediately** by the first signed
  release. The elevation prompt then shows "SignPath Foundation" as the
  verified publisher.
- **SmartScreen "Windows protected your PC / unrecognized app"** — **not**
  fixed immediately. It is reputation-based and clears only as the signed
  binaries accumulate clean downloads (Microsoft's estimate: "several weeks
  and hundreds of clean installs," with no guaranteed date and no expedite
  path short of the Microsoft Store). Paying for an EV certificate would
  **not** change this — Microsoft removed EV's instant-reputation privilege
  in 2024.

Reputation accrues to the signing **certificate identity** as well as each
file hash, so signing every release with the same SignPath identity from the
first signed build onward is what makes the warning fade fastest.

## One-time setup

### 1. Apply to SignPath Foundation
Apply at <https://signpath.org/> with the RxTerm repo. The project qualifies
(GPL-3.0 is an OSI-approved license). Approval is manual and can take a few
days. You'll be given a SignPath **organization**.

### 2. Configure the SignPath organization (SignPath web UI)
- Add the predefined **GitHub.com** trusted build system to the organization.
- Create a **project** (note its *project slug*, e.g. `rxterm`) and link the
  GitHub trusted build system to it, pointing at this repository.
- Create an **artifact configuration** describing the files to sign — a
  directory (the uploaded `unsigned/` artifact) containing the NSIS `.exe`
  and the `.msi`, each Authenticode-signed. Note its *artifact configuration
  slug*.
- Create a **signing policy** (e.g. `release-signing`) tied to that artifact
  configuration. Note its *signing policy slug*.
- Enable **origin verification** (required for the free Foundation program).

### 3. Add the policy file to this repo
SignPath's Foundation onboarding gives you the exact contents for
`.signpath/policies/<project-slug>/<signing-policy-slug>.yml`. Commit that
file to the default branch (it enforces constraints like branch protection
and no build reruns). Use the content SignPath provides verbatim — do not
hand-write it.

### 4. Add repository secrets and variables
In **Settings → Secrets and variables → Actions**:

Secret:
- `SIGNPATH_API_TOKEN` — a SignPath REST API token

Variables:
- `SIGNPATH_CONNECTOR_URL` — the connector URL shown in your SignPath UI
- `SIGNPATH_ORGANIZATION_ID` — your SignPath organization ID
- `SIGNPATH_PROJECT_SLUG` — e.g. `rxterm`
- `SIGNPATH_SIGNING_POLICY_SLUG` — e.g. `release-signing`
- `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` — from step 2
- `SIGNING_ENABLED` — set to `true` to turn the `sign-windows` job on

### 5. Test before a real release
Push a throwaway pre-release tag (e.g. `v0.6.1-rc1`) and confirm the
`sign-windows` job succeeds and the release's `.exe`/`.msi` verify as signed
(`signtool verify /pa /v file` on Windows, or check the file's Digital
Signatures tab). Only then tag a real release. If anything is misconfigured,
unset `SIGNING_ENABLED` to fall back to unsigned releases.

## Known follow-ups
- The bare `rxterm.exe` inside the portable zip is **not** signed by this job
  (it is zipped during the build, before signing). Signing it requires
  signing the binary before the portable zip is created.
- Code signing is unrelated to the Tauri **updater** signing key
  (`TAURI_SIGNING_PRIVATE_KEY`), which signs update manifests, not binaries.
