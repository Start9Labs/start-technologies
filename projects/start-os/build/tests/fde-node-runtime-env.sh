#!/bin/bash
export PATH=/node-runtime/usr/bin:/usr/sbin:/usr/bin:/sbin:/bin
fde_runtime_multiarch=$(gcc -print-multiarch)
export LD_LIBRARY_PATH="/node-runtime/usr/lib/$fde_runtime_multiarch"
