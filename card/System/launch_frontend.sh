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

# tray is slot with a PlayStation shelf beside its own. It runs in slot's
# place when it is on the card. Should it ever fail, slot starts on its own.
if [ -f "$SYS/tray" ]; then
	[ -f "$SD/tray.log" ] && mv -f "$SD/tray.log" "$SD/tray.log.1"
	log "run $SYS/tray"
	/lib/ld-linux-aarch64.so.1 "$SYS/tray" > "$SD/tray.log" 2>&1
	status=$?
	log "tray exited $status"
	[ "$status" -eq 0 ] && exit 0
fi

[ -f "$SD/slot.log" ] && mv -f "$SD/slot.log" "$SD/slot.log.1"
log "exec $SYS/slot"

exec /lib/ld-linux-aarch64.so.1 "$SYS/slot" > "$SD/slot.log" 2>&1
