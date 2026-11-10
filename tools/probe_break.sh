#!/usr/bin/env bash
set -u
PORT=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}/ports/functionalstorage
CTL=$PORT/target/horizon-cargo/fast/horizon-ctl; GAME=$PORT/target/horizon-cargo/fast/horizon-client
RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}; SOCK=/tmp/opencode/fs-break.sock
SCRATCH=$(mktemp -d /tmp/opencode/fs-break.XXXXXX)
export HORIZON_HOME=$SCRATCH/game HORIZON_LAUNCHER_HOME=$SCRATCH/launcher XDG_DATA_HOME=$SCRATCH/data XDG_CONFIG_HOME=$SCRATCH/config HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"; rm -f "$SOCK"; cd "$SCRATCH"
ctl() { "$CTL" --socket "$SOCK" "$@"; }
trap 'ctl quit >/dev/null 2>&1 || true' EXIT
ctl launch --game "$GAME" --world FSBreak --seed 13 --creative --offscreen --wait --game-dir "$SCRATCH/game" --resources "$RES" --dev-mod "$PORT/content"
ctl wait-ready 180
PX=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
PZ=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
ctl command "fill $((PX-8)) 100 $((PZ-8)) $((PX+8)) 100 $((PZ+8)) minecraft:stone" >/dev/null
ctl command "fill $((PX-8)) 101 $((PZ-8)) $((PX+8)) 110 $((PZ+8)) minecraft:air" >/dev/null
ctl tp "$PX.5" 101 "$((PZ+3)).5" >/dev/null; ctl wait 20
ctl command "kill @e[type=!minecraft:player]" >/dev/null
ctl command "gamerule spawn_mobs false" >/dev/null; ctl wait 10
X=$PX; Y=102; Z=$((PZ+1))
ctl setblock "$X" "$Y" $((Z-1)) minecraft:stone >/dev/null
ctl setblock "$X" "$Y" "$Z" "functionalstorage:oak_1[facing=south]" >/dev/null; ctl wait 10
ctl give minecraft:stone 64 >/dev/null; ctl wait 10
ctl select minecraft:stone >/dev/null || true; ctl wait 5
ctl interact "$X" "$Y" "$Z" south >/dev/null || true; ctl wait 10
info() { ctl chat "/functionalstorage info $1 $2 $3" >/dev/null; ctl wait 8 >/dev/null; ctl messages 6 | jq -r '.chat[]' | grep -m1 'functionalstorage:' || true; }
echo "before break: $(info $X $Y $Z)"
ctl command "gamemode survival" >/dev/null; ctl wait 5
ctl look-at block "$X" "$Y" "$Z" south >/dev/null; ctl wait 5
echo "break: $(ctl break $X $Y $Z 2>&1)"
ctl wait 10 >/dev/null
ctl tp "$X.5" "$Y" "$((Z+1)).5" >/dev/null; ctl wait 30 >/dev/null
ctl wait 4 >/dev/null
echo "msgs: $(ctl messages 10 | jq -c '{actionbar, chat: (.chat[:4])}')"
echo "inv: $(ctl inventory | jq -c '{counts, held}')"
echo "items: $(ctl entities 8 | jq -c '[.entities[]? | select(.kind=="item")]')"
ctl place "$X" "$Y" "$Z" --item functionalstorage:oak_1 >/dev/null 2>&1 || echo "place failed"
ctl wait 10 >/dev/null
echo "after place: $(info $X $Y $Z)"
