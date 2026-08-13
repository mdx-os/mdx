#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
command -v xcodegen >/dev/null
xcodegen generate --spec project.yml
command -v perl >/dev/null
perl -0pi -e 's/^\s*CODE_SIGN_IDENTITY = "iPhone Developer";\n//mg' \
  MDxAnywhere.xcodeproj/project.pbxproj
echo "ios_project_generate: OK project=MDxAnywhere.xcodeproj"
