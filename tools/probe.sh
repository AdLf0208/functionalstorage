#!/usr/bin/env bash
set -u
CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/functionalstorage
CTL=$PORT/target/horizon-cargo/fast/horizon-ctl
GAME=$PORT/target/horizon-cargo/fast/horizon-client
RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}
SOCK=/tmp/opencode/functionalstorage-probe.sock
SCRATCH=$(mktemp -d /tmp/opencode/functionalstorage-probe.XXXXXX)
export HORIZON_HOME=$SCRATCH/game HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data XDG_CONFIG_HOME=$SCRATCH/config HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"; rm -f "$SOCK"; cd "$SCRATCH"
ctl() { "$CTL" --socket "$SOCK" "$@"; }
end() { ctl command "execute in minecraft:the_end run $1"; }
cleanup() { ctl quit >/dev/null 2>&1 || true; sleep 1; }
trap cleanup EXIT
ctl launch --game "$GAME" --world FSProbe --seed 7 --creative --offscreen --wait \
    --game-dir "$SCRATCH/game" --resources "$RES" --dev-mod "$PORT/content"
ctl wait-ready 180
end "forceload add 992 992 1008 1008"
ctl wait 60
end "fill 992 79 992 1008 79 1008 minecraft:stone"
end "fill 992 80 992 1008 88 1008 minecraft:air"
end "tp @s 1000.5 80 1002.5"
ctl wait 20
ctl tp 1000.5 80 1002.5
ctl wait 20
ctl command "gamerule spawn_mobs false" >/dev/null
ctl wait 20
echo "state: $(ctl state | jq -c '{dimension, player: .player.position, server: .server.dimension}')"
ctl setblock 1000 80 1000 "functionalstorage:oak_1[facing=south]"
ctl wait 10
info() { ctl chat "/functionalstorage info 1000 80 1000"; ctl wait 8; ctl messages 6 | jq -r '.chat[]' | grep -m1 'functionalstorage:' || true; }
echo "empty: $(info)"
ctl command "clear @s" >/dev/null 2>&1
ctl give minecraft:stone 100 >/dev/null 2>&1
ctl wait 10
ctl select minecraft:stone >/dev/null 2>&1 || true
echo "held-before: $(ctl inventory | jq -c '.held')"
echo "interact: $(ctl interact 1000 80 1000 south 2>&1)"
ctl wait 10
echo "held-after: $(ctl inventory | jq -c '.held')"
echo "counts: $(ctl inventory | jq -c '.counts')"
echo "after: $(info)"
echo "screen: $(ctl screen 2>&1 | cut -c1-500)"
echo "menu: $(ctl menu 2>&1 | cut -c1-500)"
ctl setblock 1000 80 1000 minecraft:air >/dev/null
ctl setblock 1001 80 1000 minecraft:air >/dev/null
