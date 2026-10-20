# Runs the same checks as .github/workflows/ci.yml locally (Windows). Requires Git Bash on PATH.
$ErrorActionPreference = 'Stop'
& bash "$PSScriptRoot/local-ci.sh"
exit $LASTEXITCODE
