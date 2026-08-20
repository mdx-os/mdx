#!/usr/bin/env bash

set -euo pipefail

# GitHub's Ubuntu runners use an Azure mirror list that can occasionally
# accept a connection without returning package indexes. Keep every apt
# operation bounded and prefer Ubuntu's HTTPS archive when that list exists.
if sudo test -f /etc/apt/apt-mirrors.txt; then
  printf '%s\n' 'https://archive.ubuntu.com/ubuntu' |
    sudo tee /etc/apt/apt-mirrors.txt >/dev/null
fi

exec sudo env DEBIAN_FRONTEND=noninteractive apt-get \
  -o Acquire::Retries=3 \
  -o Acquire::http::Timeout=15 \
  -o Acquire::https::Timeout=15 \
  -o Acquire::ForceIPv4=true \
  "$@"
