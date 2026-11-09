#!/usr/bin/env bash
# Installs apt packages, reusing the .debs in ~/apt-debs (restored by
# actions/cache) and leaving every .deb there for the cache to save. The
# runner's apt mirror sometimes crawls (one job took 5.5 min to fetch
# ~60 MB), so CI caches the downloads per runner image.
#
# Usage: apt-install-cached.sh [apt-get install options] package...
set -euo pipefail

# The runner is throwaway: skip the man-db rebuild and dpkg's fsyncs.
sudo rm -f /var/lib/man-db/auto-update

mkdir -p ~/apt-debs
sudo cp ~/apt-debs/*.deb /var/cache/apt/archives/ 2>/dev/null || true
sudo apt-get update
sudo apt-get install -y -o APT::Keep-Downloaded-Packages=true -o Dpkg::Options::=--force-unsafe-io "$@"
cp /var/cache/apt/archives/*.deb ~/apt-debs/
