# Official Diagram Visual Regression

This folder contains the official diagram rendering cases and the Playwright-based visual workflow for Supramark.

## Layout

- `cases/official-diagram-rendering-cases.md`: primary official cases.
- `cases/official-diagram-rendering-cases-v2.md`: optional extended cases.
- `cases/assets/`: official reference SVGs used by the issue reports.
- `scripts/tools/official-diagram-visual-workflow.mjs`: runner.
- `artifacts/`: generated output, not committed.

## Local Run

```powershell
cd tests/official-diagram-visual
npm ci
$env:SOURCE_DOCS='cases/official-diagram-rendering-cases.md'
$env:CASE_IDS='all'
$env:SUBMIT_GITHUB_ISSUES='0'
$env:PLAYWRIGHT_HEADLESS='0'
npm run visual:official-diagrams
```

Current-run outputs:

- `artifacts/official-diagram-visual-workflow/summary.json`
- `artifacts/official-diagram-visual-workflow/report.html`
- `artifacts/official-diagram-visual-workflow/CURRENT_RUN_ARTIFACTS.json`
- `artifacts/official-diagram-visual-workflow/issues/CURRENT_ISSUES.md`

Only files listed in `CURRENT_RUN_ARTIFACTS.json` should be considered current for a run.

## GitHub Actions

The workflow entry is stored at:

```text
.github/workflows/official-diagram-visual-regression.yml
```

Run it manually from GitHub Actions with:

- `case_ids`: `all`
- `source_docs`: `cases/official-diagram-rendering-cases.md`
- `supramark_url`: `https://actrium.github.io/supramark/playground/`
- `issue_repo`: `Actrium/supramark`
- `submit_github_issues`: `0`
- `playwright_headless`: `1`
- `playwright_viewport`: `1280x900`

Issue creation is opt-in. Set `submit_github_issues` to `1` to create issues for
failed cases. `pass` and `review` cases are not submitted.
