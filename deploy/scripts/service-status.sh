#!/usr/bin/env bash
# Show cce-server service status and basic paths.
set -euo pipefail

SCOPE="user"
[[ "${1:-}" == "--system" ]] && SCOPE="system"

if [[ "$SCOPE" == "system" ]]; then
    systemctl status cce-server.service --no-pager || true
    journalctl -u cce-server.service -n 50 --no-pager || true
else
    systemctl --user status cce-server.service --no-pager || true
    journalctl --user -u cce-server.service -n 50 --no-pager || true
fi
