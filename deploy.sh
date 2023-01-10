#!/bin/sh

set -o xtrace
set -o errexit
set -o nounset


sudo cp ./monitoring.service /etc/systemd/system/monitoring.service

sudo systemctl daemon-reload
sudo systemctl enable monitoring.service
sudo systemctl start monitoring.service