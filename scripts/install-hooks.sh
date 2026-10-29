#!/usr/bin/env bash
# Runs the fast checks before each commit and the full gate before each push.
set -euo pipefail
hooks="$(git rev-parse --git-path hooks)"
printf '#!/usr/bin/env bash\nexec scripts/check.sh --fast\n' > "$hooks/pre-commit"
printf '#!/usr/bin/env bash\nexec scripts/check.sh\n' > "$hooks/pre-push"
chmod +x "$hooks/pre-commit" "$hooks/pre-push"
echo "Installed pre-commit (fast checks) and pre-push (full gate) hooks."
