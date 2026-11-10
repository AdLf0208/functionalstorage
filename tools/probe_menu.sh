#!/usr/bin/env bash
set -u
PORT=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}/ports/functionalstorage
CTL=$PORT/target/horizon-cargo/fast/horizon-ctl; GAME=$PORT/target/horizon-cargo/fast/horizon-client
RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}; SOCK=/tmp/opencode/fs-menu.sock
SCRATCH=$(mktemp -d /tmp/opencode/fs-menu.XXXXXX)
export HORIZON_HOME=$SCRATCH/game HORIZON_LAUNCHER_HOME=$SCRATCH/launcher XDG_DATA_HOME=$SCRATCH/data XDG_CONFIG_HOME=$SCRATCH/config HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"; rm -f "$SOCK"; cd "$SCRATCH"
ctl() { "$CTL" --socket "$SOCK" "$@"; }
trap 'ctl quit >/dev/null 2>&1 || true' EXIT
ctl launch --game "$GAME" --world FSMenu --seed 11 --creative --offscreen --wait --game-dir "$SCRATCH/game" --resources "$RES" --dev-mod "$PORT/content"
ctl wait-ready 180
PX=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
PZ=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
ctl command "fill $((PX-8)) 100 $((PZ-8)) $((PX+8)) 100 $((PZ+8)) minecraft:stone" >/dev/null
ctl command "fill $((PX-8)) 101 $((PZ-8)) $((PX+8)) 110 $((PZ+8)) minecraft:air" >/dev/null
ctl tp "$PX.5" 101 "$((PZ+3)).5" >/dev/null; ctl wait 20
ctl command "kill @e[type=!minecraft:player]" >/dev/null
ctl command "gamerule spawn_mobs false" >/dev/null; ctl wait 10
ctl setblock "$PX" 102 "$((PZ+1))" "functionalstorage:oak_1[facing=south]" >/dev/null; ctl wait 10
ctl command "clear @s" >/dev/null 2>&1; ctl wait 5
echo "held: $(ctl inventory | jq -c '.held')"
echo "look: $(ctl look-at block "$PX" 102 "$((PZ+1))" south 2>&1)"
echo "interact: $(ctl interact "$PX" 102 "$((PZ+1))" south 2>&1)"
ctl wait 10
echo "screen: $(ctl screen 2>&1 | cut -c1-600)"
echo "menu: $(ctl menu 2>&1 | cut -c1-800)"
ctl chat "/functionalstorage info $PX 102 $((PZ+1))" >/dev/null; ctl wait 8 >/dev/null
echo "msgs: $(ctl messages 8 | jq -c '.chat')"
