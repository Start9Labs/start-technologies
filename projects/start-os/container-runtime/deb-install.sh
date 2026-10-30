#!/bin/bash

set -e

apt-get update
apt-get upgrade -y
apt-get install -y curl rsync qemu-user-static nodejs

cp /usr/lib/startos/resolved.conf /etc/systemd/resolved.conf
sed -i '/\(^\|#\)Storage=/c\Storage=persistent' /etc/systemd/journald.conf
sed -i '/\(^\|#\)Compress=/c\Compress=yes' /etc/systemd/journald.conf
sed -i '/\(^\|#\)SystemMaxUse=/c\SystemMaxUse=1G' /etc/systemd/journald.conf
sed -i '/\(^\|#\)ForwardToSyslog=/c\ForwardToSyslog=no' /etc/systemd/journald.conf

systemctl enable container-runtime.service

echo "nameserver 10.0.3.1" > /etc/resolv.conf