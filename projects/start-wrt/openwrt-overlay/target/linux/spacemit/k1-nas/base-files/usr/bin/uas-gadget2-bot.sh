#!/bin/sh
# configfs for bot mass-storage usb gadget
# should enable in Kconfig: target core, target core > fileio, f_msc
name=$(basename $0)
VENDOR_ID="0x361C"
PRODUC_ID="0x002f"
SERNUM_STR="20211102"
MANUAF_STR="Spacemit"
PRODUC_STR="K1 Mass Storage(BOT)"
# SCSI naa
CONFIGFS=/sys/kernel/config
GADGET_PATH=$CONFIGFS/usb_gadget/msc
GFUNC_PATH=$GADGET_PATH/functions
GCONFIG=$GADGET_PATH/configs/c.1
usage()
{
	echo "$name Usage: "
	echo ""
	echo -e "\texample: $name start /dev/mmcblk1p1"
	echo -e "\texample: $name stop"
	echo ""
}
########################### Gadget ####################################msc#######
gadget_info()
{
	echo "$name: $1"
}
gadget_debug()
{
	[ $DEBUG ] && echo "$name: $1"
}
die()
{
	gadget_info "$1"
	exit 1
}
g_remove()
{
	[ -h $1 ] && rm -f $1
	[ -d $1 ] && rmdir $1
	[ -e $1 ] && rm -f $1
}
enable_udc()
{
	echo c0900100.udc > $GADGET_PATH/UDC
}
stop()
{
	[ -e $GADGET_PATH/UDC ] || die "gadget not configured, no need to clean"
	gadget_info "Echo none to udc"
	[ -e $GADGET_PATH/UDC ] || die "gadget not configured yet"
	[ `cat $GADGET_PATH/UDC` ] && echo "" > $GADGET_PATH/UDC
	gadget_debug "clean msc"
	gadget_debug "remove msc from usb config"
	g_remove $GCONFIG/mass_storage.usb0
	# Remove strings:
	gadget_info "remove strings of c.1."
	g_remove $GCONFIG/strings/0x409
	# Remove config:
	gadget_info "remove configs c.1."
	g_remove $GCONFIG
	g_remove $GFUNC_PATH/mass_storage.usb0
	# Remove string in gadget
	gadget_info "remove strings of $GADGET_PATH."
	g_remove $GADGET_PATH/strings/0x409
	# Remove gadget
	gadget_info "remove $GADGET_PATH."
	g_remove $GADGET_PATH
}
check()
{
	DEVICE=$1
	[ -n "$DEVICE" ] || die "No device specificed"
	[ -b $DEVICE -o -f $DEVICE ] || die "Invalid device or file: ${DEVICE}"
}
start()
{
	DEVICE=$1
	gadget_info "config $VENDOR_ID/$PRODUC_ID/$SERNUM_STR/$MANUAF_STR/$PRODUC_STR."
	mountpoint -q /sys/kernel/config || mount -t configfs none /sys/kernel/config
	[ -e $GADGET_PATH ] && die "ERROR: gadget already configured, should run stop first"
	mkdir $GADGET_PATH
	echo $VENDOR_ID > $GADGET_PATH/idVendor
	echo $PRODUC_ID > $GADGET_PATH/idProduct
	mkdir $GADGET_PATH/strings/0x409
	echo $SERNUM_STR > $GADGET_PATH/strings/0x409/serialnumber
	echo $MANUAF_STR > $GADGET_PATH/strings/0x409/manufacturer
	echo $PRODUC_STR > $GADGET_PATH/strings/0x409/product
	mkdir $GCONFIG
	echo 0xc0 > $GCONFIG/bmAttributes
	echo 500 > $GCONFIG/MaxPower
	mkdir $GCONFIG/strings/0x409
	gadget_debug "add a msc function instance"
	MSC_DIR=$GFUNC_PATH/mass_storage.usb0
	mkdir -p $MSC_DIR
	echo $DEVICE >  $MSC_DIR/lun.0/file
	echo 1 > $MSC_DIR/lun.0/removable
	echo 0 > $MSC_DIR/lun.0/nofua
	gadget_debug "add msc to usb config"
	ln -s $MSC_DIR $GCONFIG/mass_storage.usb0
	enable_udc
}
############################ MAIN #############################################
case "$1" in
	stop)
		stop
		;;
	start)
		check $2
		start $2
		;;
	*)
		usage
		;;
esac
exit $?

