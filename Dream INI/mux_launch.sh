#!/bin/sh
# HELP: Import Morrowind.ini settings into OpenMW configuration.
# ICON: task
# GRID: Dream INI
. /opt/muos/script/var/func.sh
APP_BIN="dream-ini"
SETUP_APP "$APP_BIN" "modern"
APP_DIR="/mnt/mmc/MUOS/application/Dream INI"
LOG_DIR="$APP_DIR/logs"
mkdir -p "$LOG_DIR"
cd "$APP_DIR" || exit 1
"./$APP_BIN" >"$LOG_DIR/dream-ini.log" 2>&1
