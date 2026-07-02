#!/bin/bash
# NVMe Isolation Test Script
# Automates Helix's requested diagnostic data collection for Issue #3336
# Usage: ./nvme-isolation-test.sh [baseline | apst | hmb | aspm | collect]

set -e

if [ "$(id -u)" -ne 0 ]; then
    echo "This script must be run as root." >&2
    exit 1
fi

COMMAND=$1

case $COMMAND in
    collect)
        echo "=== Collecting Drive & Controller State ==="
        echo "1. HMB Request Status:"
        nvme id-ctrl /dev/nvme0 | grep -iE 'hmb|hmpre|hmmin' || echo "Failed to get HMB info"
        
        echo -e "\n2. Current APST Table/Config:"
        nvme get-feature /dev/nvme0 -f 0x0c -H || echo "Failed to get APST config"
        
        echo -e "\n3. Current Power-Management State:"
        nvme get-feature /dev/nvme0 -f 0x02 -H || echo "Failed to get PM state"
        
        echo -e "\n4. PCIe Link Power State (ASPM):"
        NVME_BUS=$(lspci | grep -i nvme | awk '{print $1}')
        if [ -n "$NVME_BUS" ]; then
            lspci -vvv -s "$NVME_BUS" | grep -iE 'LnkCtl|ASPM' || echo "Failed to get ASPM info"
        else
            echo "Could not identify NVMe bus address"
        fi
        
        echo -e "\n5. Kernel params:"
        uname -a
        if [ -f /sys/module/nvme_core/parameters/default_ps_max_latency_us ]; then
            echo -n "nvme_core.default_ps_max_latency_us="
            cat /sys/module/nvme_core/parameters/default_ps_max_latency_us
        fi
        
        echo -e "\n6. Smart Log:"
        nvme smart-log /dev/nvme0 || echo "Failed to get smart log"
        exit 0
        ;;
    baseline)
        echo "Setting GRUB to Baseline (No extra params)..."
        sed -i 's/ nvme_core\.default_ps_max_latency_us=0//g' /etc/default/grub
        sed -i 's/ nvme\.max_host_mem_size_mb=0//g' /etc/default/grub
        sed -i 's/ pcie_aspm=off//g' /etc/default/grub
        ;;
    apst)
        echo "Setting GRUB to disable APST only..."
        $0 baseline
        sed -i 's/GRUB_CMDLINE_LINUX="/GRUB_CMDLINE_LINUX="nvme_core.default_ps_max_latency_us=0 /' /etc/default/grub
        ;;
    hmb)
        echo "Setting GRUB to disable HMB only..."
        $0 baseline
        sed -i 's/GRUB_CMDLINE_LINUX="/GRUB_CMDLINE_LINUX="nvme.max_host_mem_size_mb=0 /' /etc/default/grub
        ;;
    aspm)
        echo "Setting GRUB to disable ASPM only..."
        $0 baseline
        sed -i 's/GRUB_CMDLINE_LINUX="/GRUB_CMDLINE_LINUX="pcie_aspm=off /' /etc/default/grub
        ;;
    *)
        echo "Usage: $0 [baseline | apst | hmb | aspm | collect]"
        echo "  collect  - Gather controller state information"
        echo "  baseline - Remove all NVMe workaround kernel parameters"
        echo "  apst     - Apply only nvme_core.default_ps_max_latency_us=0"
        echo "  hmb      - Apply only nvme.max_host_mem_size_mb=0"
        echo "  aspm     - Apply only pcie_aspm=off"
        exit 1
        ;;
esac

echo "Updating GRUB..."
if command -v update-grub &> /dev/null; then
    update-grub
else
    # Fallback for systems that might use grub-mkconfig directly
    grub-mkconfig -o /boot/grub/grub.cfg
fi

echo "Done! Please reboot and run the Bitcoin IBD to test if the reset occurs."
