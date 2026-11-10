#!/usr/bin/env bash
set -u
PORT=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}/ports/functionalstorage
CTL=$PORT/target/horizon-cargo/fast/horizon-ctl
GAME=$PORT/target/horizon-cargo/fast/horizon-client
RES=${HORIZON_RES:-$HOME/.local/horizon-client/26.3}
SOCK=/tmp/opencode/fs-place.sock
SCRATCH=$(mktemp -d /tmp/opencode/fs-place.XXXXXX)
export HORIZON_HOME=$SCRATCH/game HORIZON_LAUNCHER_HOME=$SCRATCH/launcher XDG_DATA_HOME=$SCRATCH/data XDG_CONFIG_HOME=$SCRATCH/config HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"; rm -f "$SOCK"; cd "$SCRATCH"
ctl() { "$CTL" --socket "$SOCK" "$@"; }
end() { ctl command "execute in minecraft:the_end run $1"; }
trap 'ctl quit >/dev/null 2>&1 || true' EXIT
ctl launch --game "$GAME" --world FSPlace --seed 9 --creative --offscreen --wait --game-dir "$SCRATCH/game" --resources "$RES" --dev-mod "$PORT/content"
ctl wait-ready 180
end "forceload add 992 992 1008 1008"; ctl wait 60
end "fill 992 79 992 1008 79 1008 minecraft:stone"
end "fill 992 80 992 1008 88 1008 minecraft:air"
end "tp @s 1000.5 80 1002.5"; ctl wait 20; ctl tp 1000.5 80 1002.5; ctl wait 20
end "setblock 1000 82 1000 minecraft:stone"
ctl command "gamerule spawn_mobs false" >/dev/null
ctl wait 20
echo "state: $(ctl state | jq -c '{dim, p: .player.position, yaw, pitch, t: .target}')"
echo "support: $(ctl block 1000 82 1000)"
echo "look: $(ctl look-at block 1000 82 1000 south 2>&1)"
echo "looking: $(ctl looking-at 2>&1)"
ctl give functionalstorage:oak_1 1 >/dev/null 2>&1
ctl wait 10
echo "inv: $(ctl inventory | jq -c '{counts, held}')"
echo "place: $(ctl place 1000 82 1001 --item functionalstorage:oak_1 2>&1)"
ctl wait 10
echo "after: $(ctl block 1000 82 1001)"
echo "inv2: $(ctl inventory | jq -c '{counts, held}')"
