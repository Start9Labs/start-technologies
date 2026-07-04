#!/bin/sh
# configfs for uas mass-storage usb gadget
# should enable in Kconfig: target core, target core > fileio, f_tcm
name=$(basename $0)
VENDOR_ID="0x361C"
PRODUC_ID="0x001f"
SERNUM_STR="20211102"
MANUAF_STR="Spacemit"
PRODUC_STR="K1 Mass Storage(UASP)"
# SCSI naa
NAA="naa.6001405c3214b06a"
CONFIGFS=/sys/kernel/config
CORE_DIR=$CONFIGFS/target/core
USB_GDIR=$CONFIGFS/target/usb_gadget
GADGET_PATH=$CONFIGFS/usb_gadget/tcm
die()
{
    echo $1
    exit 1
}
usage()
{
    echo "$name Usage: "
    echo ""
    echo "$name start [device]: start tcm usb_gadget"
    echo -e "\t\tdevice: block dev to use iblock, file to use fileio, \`rd\` to use rd_mcp"
    echo -e "\t\tstop: stop tcm usb_gadget"
    echo -e "\texample: $name /dev/mmcblk1p1"
    echo ""
}
########################### Gadget ####################################tcm#######
enable_udc()
{
    # Ensure fu->tpg = tpg_instances[i].tpg won't get NULL
    ln -s $GADGET_PATH/functions/tcm.0 $GADGET_PATH/configs/c.1/
    # If only one usb controller:
    echo c0a00000.dwc3 > $GADGET_PATH/UDC
}
config_gadget()
{
    echo "gadget-setup: config $VENDOR_ID/$PRODUC_ID/$SERNUM_STR/$MANUAF_STR/$PRODUC_STR."
    mkdir $GADGET_PATH
    echo $VENDOR_ID > $GADGET_PATH/idVendor
    echo $PRODUC_ID > $GADGET_PATH/idProduct
    mkdir $GADGET_PATH/strings/0x409
    echo $SERNUM_STR > $GADGET_PATH/strings/0x409/serialnumber
    echo $MANUAF_STR > $GADGET_PATH/strings/0x409/manufacturer
    echo $PRODUC_STR > $GADGET_PATH/strings/0x409/product
    mkdir $GADGET_PATH/configs/c.1
    echo 0xc0 > $GADGET_PATH/configs/c.1/bmAttributes
    echo 2 > $GADGET_PATH/configs/c.1/MaxPower
    mkdir $GADGET_PATH/configs/c.1/strings/0x409
    mkdir -p $GADGET_PATH/functions/tcm.0
}
clean_gadget()
{
    rmdir $GADGET_PATH/functions/tcm.0
    # Remove strings:
    rmdir $GADGET_PATH/configs/c.1/strings/0x409
    rmdir $GADGET_PATH/strings/0x409
    # Remove config:
    rmdir $GADGET_PATH/configs/c.1
    rmdir $GADGET_PATH
}
disable_udc()
{
    echo  > $GADGET_PATH/UDC
    rm -f $GADGET_PATH/configs/c.1/tcm.0
}
############################### TCM related ##################################

config_target()
{
    DEVICE=$1
    # Create a backstore
    if [ -z "$DEVICE" ]; then
        # RD_MCP backend is only for DEBUG usage
        echo "$name: no device specificed, select rd_mcp as backstore"
        BACKSTORE_DIR=$CORE_DIR/rd_mcp_0/ramdisk
        mkdir -p $BACKSTORE_DIR
        # 128MB pure ramdisk
        echo rd_pages=32768 > $BACKSTORE_DIR/control
    elif [ -b $DEVICE ]; then
        echo "$name: block device, select iblock as backstore"
        BACKSTORE_DIR=$CORE_DIR/iblock_0/iblock
        mkdir -p $BACKSTORE_DIR
        echo "udev_path=${DEVICE}" > $BACKSTORE_DIR/control
    else
        echo "$name: other path, select fileio as backstore"
        BACKSTORE_DIR=$CORE_DIR/fileio_0/fileio
        mkdir -p $BACKSTORE_DIR
        DEVICE_SIZE=$(du -b $DEVICE | cut -f1)
        echo "fd_dev_name=${DEVICE},fd_dev_size=${DEVICE_SIZE}" > $BACKSTORE_DIR/control
        # echo 1 > $BACKSTORE_DIR/attrib/emulate_write_cache
    fi
    [ -n "$DEVICE" ] && umount $DEVICE
    echo 1 > $BACKSTORE_DIR/enable
    echo "$name: NAA of target: $NAA"
    # Create an NAA target and a target portal group (TPG)
    mkdir -p $USB_GDIR/$NAA/tpgt_1/
    echo "$name tpgt_1 has lun_0"
    # Create a LUN
    mkdir $USB_GDIR/$NAA/tpgt_1/lun/lun_0
    # Nexus initiator on target port 1 to $NAA
    echo $NAA > $USB_GDIR/$NAA/tpgt_1/nexus
    # Allow write access for non authenticated initiators
    # echo 0 > $USB_GDIR/$NAA/tpgt_1/attrib/demo_mode_write_protect
    ln -s $BACKSTORE_DIR $USB_GDIR/$NAA/tpgt_1/lun/lun_0/data
    #ln -s $BACKSTORE_DIR $USB_GDIR/$NAA/tpgt_1/lun/lun_0/virtual_scsi_port
    # Enable the target portal group, with 1 lun
    echo 1 > $USB_GDIR/$NAA/tpgt_1/enable
}
clean_target()
{
    [ -d "$USB_GDIR/$NAA/tpgt_1/enable" ] && echo 0 > $USB_GDIR/$NAA/tpgt_1/enable
    rm -f $USB_GDIR/$NAA/tpgt_1/lun/lun_0/data
    rm -f $USB_GDIR/$NAA/tpgt_1/lun/lun_0/virtual_scsi_port
    rmdir $USB_GDIR/$NAA/tpgt_1/lun/lun_0
    rmdir $USB_GDIR/$NAA/tpgt_1/
    rmdir $USB_GDIR/$NAA/
    rmdir $USB_GDIR
    BACKSTORE_DIR=$CORE_DIR/iblock_0/iblock
    rmdir $BACKSTORE_DIR
    BACKSTORE_DIR=$CORE_DIR/fileio_0/fileio
    rmdir $BACKSTORE_DIR
    BACKSTORE_DIR=$CORE_DIR/rd_mcp_0/ramdisk
    rmdir $BACKSTORE_DIR
}
stop()
{
    disable_udc
    clean_target
    clean_gadget
}
check()
{
    DEVICE=$1
    [ -n "$DEVICE" ] || die "No device specificed"
    [ -b $DEVICE -o -f $DEVICE ] || die "Invalid device or file: ${DEVICE}"
}
start()
{
    # Load the target modules and mount the config file system
    # Uncomment these if modules not built-in:
    # lsmod | grep -q configfs || modprobe configfs
    # lsmod | grep -q target_core_mod || modprobe target_core_mod
    DEVICE=$1
    mount | grep configfs
    [ $? -eq 0 ] || mount -t configfs none $CONFIGFS
    mkdir -p $USB_GDIR
    config_gadget
    # Config target after gadget, or tpgt cannot be mkdired
    config_target $DEVICE
    enable_udc
}
############################ MAIN #############################################
case "$1" in
    stop)
        stop
        ;;
    start)
        case "$2" in
            rd|ramdisk|rd_mcp)
                start
                ;;
            *)
                check $2
                start $2
                ;;
        esac
        ;;
    *)
        usage
        ;;
esac
exit $?