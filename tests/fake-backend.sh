#!/bin/sh
# Isolated transport fixture for ServiceTest.qml; never invokes real tools.
set -eu
work=$1
shift
case "$1" in
  diagnostics)
    if [ -f "$work/diagnostics" ]; then touch "$work/repeated-diagnostics"; fi
    touch "$work/diagnostics"
    printf '%s\n' '{"setupStatus":"Ready","backgroundChecksEnabled":true,"backgroundChecksActive":true}'
    ;;
  status)
    running=true
    timers=1
    status=idle
    if [ -f "$work/uploading" ]; then status=uploading; fi
    if [ -f "$work/stopped" ]; then running=false; timers=0; fi
    if [ -f "$work/synced" ]; then status=synced; fi
    printf '{"state":{"projects":[],"drive":{"syncOnStartup":false}},"activeTasks":[{"id":"tracked","running":%s,"displaySeconds":0}],"runningTimers":%s,"nowMs":%s,"syncStatus":"%s"}\n' \
      "$running" "$timers" "$(date +%s000)" "$status"
    ;;
  sync)
    touch "$work/uploading"
    # A single shared queue deadlocks here: stop must run before sync can end.
    while [ ! -f "$work/stopped" ]; do sleep 0.02; done
    touch "$work/synced"
    ;;
  task)
    test "$2" = stop
    test -f "$work/uploading"
    touch "$work/stopped"
    ;;
  report)
    touch "$work/unexpected-report-check"
    ;;
  verify)
    test -f "$work/stopped"
    test -f "$work/synced"
    test -f "$work/diagnostics"
    test ! -f "$work/repeated-diagnostics"
    test ! -f "$work/unexpected-report-check"
    touch "$work/passed"
    ;;
  *) exit 1 ;;
esac
