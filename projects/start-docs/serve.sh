#!/bin/bash
set -e

cd "$(dirname "$0")"

# Build all books
./build.sh

# Serve docs directory
BOOK="${1:-start-os}"
echo "Serving from docs/ at http://localhost:3000"
echo "  Landing:     http://localhost:3000/"
echo "  StartOS:     http://localhost:3000/start-os/"
echo "  StartTunnel: http://localhost:3000/start-tunnel/"
echo "  Packaging:   http://localhost:3000/packaging/"
echo "  StartWRT:    http://localhost:3000/start-wrt/"
echo ""
echo "To live-reload a single book while editing, run in another terminal:"
echo "  cd $BOOK && mdbook serve -p 3001"
echo ""
python3 -m http.server 3000 -d docs
