#!/bin/sh
SD="${SLOT_ROOT:-/mnt/sdcard}"
SYS="$SD/System"
RUN="${AGS_RUN:-/run}"
export SLOT_ROOT="$SD"

log() {
	echo "$(date '+%H:%M:%S') $*" >> "$SD/slot-session.log" 2>/dev/null
}

pid="$(cat "$RUN/slot-led.pid" 2>/dev/null)"
if [ -z "$pid" ] || [ ! -d "/proc/$pid" ]; then
	sh "$SYS/led.sh" once
	sh "$SYS/led.sh" loop > /dev/null 2>&1 &
	echo $! > "$RUN/slot-led.pid"
fi

echo "|/bin/sh $SYS/coresave.sh %p %s %e %t" > /proc/sys/kernel/core_pattern 2>/dev/null

# tray's PlayStation probe runs first while a PLAY-PSX file is on the card,
# and slot starts as usual once it quits (hold MENU, press START).
if [ -f "$SD/PLAY-PSX" ] && [ -f "$SYS/tray-psx" ]; then
	[ -f "$SD/tray-psx.log" ] && mv -f "$SD/tray-psx.log" "$SD/tray-psx.log.1"
	log "run $SYS/tray-psx"
	/lib/ld-linux-aarch64.so.1 "$SYS/tray-psx" > "$SD/tray-psx.log" 2>&1
	log "tray-psx exited $?"
fi

[ -f "$SD/slot.log" ] && mv -f "$SD/slot.log" "$SD/slot.log.1"
log "exec $SYS/slot"

exec /lib/ld-linux-aarch64.so.1 "$SYS/slot" > "$SD/slot.log" 2>&1
