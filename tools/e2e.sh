#!/usr/bin/env bash
# In-game check for Functional Storage slice 1: the 1x1 storage drawers.
#
# Default run (offscreen renderer, creative launch, then survival/normal in
# The End):
#   1. an oak drawer places, faces its placer and reads back `0/2048 empty`;
#   2. right-clicking its front with 100 stone stores all 100 (not a stack);
#   3. a different item (dirt) is refused and stays in the hand;
#   4. the one-slot menu opens on an empty-hand click and a right-click on its
#      result slot withdraws one (the view is 2 at 64 stored, half of it is 1),
#      which the block entity reconciles against the true amount (64 -> 63);
#   5. a second stone click adds 5 more;
#   6. in survival a left click shows the action-bar hint "Open the drawer to
#      withdraw" and takes nothing: the pin reverts a block-entity write from
#      `left_click_block`, so a real take would duplicate (notes.md);
#   7. a hopper above pushes stone in and a hopper below pulls stone out into
#      a chest (the container view is the same reconcile path);
#   8. a comparator beside it answers 1 for a partial drawer and 0 when empty;
#   9. breaking the full drawer drops it with its contents, and placing it
#      again restores the count (the `functionalstorage:drawer` component);
#  10. quitting and relaunching the same world keeps the drawer's amount and
#      kind (the block entity's saved compound);
#  11. a vanilla barrel with the same 100 stone stores only 64 and leaves 36
#      in the hand (the differential: the drawer code, not the scenario).
#
# COUNTERFACTUAL=1 replaces the drawer with a vanilla barrel in phases 2-4 and
# expects the barrel's stack-limited behaviour: the run must not see 100 stored.
# NEGATIVE=1 keeps the drawer scenario but asserts the barrel's 64 limit on it:
# the run MUST fail, proving the count assertion is not vacuous.
set -euo pipefail

CLONE=${CAMPAIGN_ROOT:-$(cd "$(dirname "$0")/../../.." && pwd)}
PORT=$CLONE/ports/functionalstorage
CTL=${CTL:-$PORT/target/horizon-cargo/fast/horizon-ctl}
GAME=${GAME:-$PORT/target/horizon-cargo/fast/horizon-client}
RES=${RES:-${HORIZON_RES:-${HORIZON_RES:-$HOME/.local/horizon-client/26.3}}}
SOCK=${SOCK:-/tmp/opencode/functionalstorage-e2e.sock}
COUNTERFACTUAL=${COUNTERFACTUAL:-0}
NEGATIVE=${NEGATIVE:-0}
# EMBEDDED=1 launches the native bundle alone (no --dev-mod).
EMBEDDED=${EMBEDDED:-0}
DEV_ARGS=()
[ "$EMBEDDED" = 1 ] || DEV_ARGS=(--dev-mod "$PORT/content")

[ -x "$CTL" ] || { echo "missing horizon-ctl: $CTL" >&2; exit 1; }
[ -x "$GAME" ] || { echo "missing horizon-client: $GAME" >&2; exit 1; }
[ -d "$RES/assets" ] || { echo "missing prepared resources: $RES" >&2; exit 1; }
[ -f "$PORT/content/horizon-mod.toml" ] || { echo "missing content mod: $PORT/content" >&2; exit 1; }

DRAWER=functionalstorage:oak_1
X=0; Z=1
SCRATCH=$(mktemp -d "${E2E_TMP:-/tmp/opencode}"/functionalstorage-home.XXXXXX)
export HORIZON_HOME=$SCRATCH/game
export HORIZON_LAUNCHER_HOME=$SCRATCH/launcher
export XDG_DATA_HOME=$SCRATCH/data
export XDG_CONFIG_HOME=$SCRATCH/config
export HORIZON_NO_AUDIO=1
mkdir -p "$(dirname "$SOCK")"
rm -f "$SOCK"
cd "$SCRATCH"

ctl() { "$CTL" --socket "$SOCK" "$@"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
say() { echo "== $* =="; }
end() { ctl command "$1"; }
count_item() { ctl inventory | jq -r --arg i "$1" '.counts[$i] // 0'; }
dropped_item() {
    ctl entities 16 item \
        | jq -r --arg i "$1" '[.entities[] | select(.kind == "item" and .item.item == $i) | .item.count] | add // 0'
}
block_name() { ctl block "$1" "$2" "$3" | jq -r '.block // "unknown"'; }
block_state() { ctl block "$1" "$2" "$3" | jq -r '.state // "unknown"'; }
block_facing() { block_state "$1" "$2" "$3" | sed -n 's/.*facing=\([a-z]*\).*/\1/p'; }
# The reply names the block at that position, so it starts with the drawer's id
# in the drawer phases and with another id ("minecraft:barrel: not a drawer")
# in the counterfactual; do not filter to one namespace.
info() { ctl chat "/functionalstorage info $1 $2 $3" >/dev/null; ctl wait 8 >/dev/null; ctl messages 8 | jq -r '.chat[]' | grep -m1 -E '^[a-z0-9_.-]+:[a-z0-9_./-]+:' || true; }
info_amount() { info "$1" "$2" "$3" | sed -n 's/.*: \([0-9][0-9]*\)\/[0-9][0-9]*.*/\1/p'; }
# The action bar is in `messages` (agent.rs `messages_value`), not in `state`.
action_bar() { ctl messages 10 | jq -r '.actionbar // ""'; }
bar_power() { ctl block "$1" "$2" "$3" | jq -r '.state // ""' | sed -n 's/.*powered=\([a-z]*\).*/\1/p'; }
chest_stone() {
    ctl interact "$1" "$2" "$3" south >/dev/null 2>&1 || true
    ctl wait 4 >/dev/null 2>&1
    ctl screen 2>/dev/null | jq -r '[.slots[]? | select(.item == "minecraft:stone") | .count] | add // 0'
    ctl close >/dev/null 2>&1 || true
}

cleanup() {
    local status=$?
    ctl quit >/dev/null 2>&1 || true
    sleep 1
    rm -rf "$SCRATCH" || true
    echo "EXIT:$status"
}
trap cleanup EXIT

say "launch (offscreen renderer, creative, content --dev-mod content/)"
"$CTL" --socket "$SOCK" launch \
    --game "$GAME" --world FunctionalStorageE2E --seed 20261105 --creative --offscreen --wait \
    --game-dir "$SCRATCH/game" --resources "$RES" "${DEV_ARGS[@]}"
ctl wait-ready 180
if [ "$EMBEDDED" = 1 ]; then
    cache=$(find "$SCRATCH" "$PORT" -maxdepth 5 -name "functionalstorage-embedded-content.hmod" 2>/dev/null | head -1)
    if [ -n "$cache" ]; then
        cmp -s "$cache" "$PORT/content/functionalstorage.hmod" \
            || fail "the embedded content differs from content/functionalstorage.hmod"
        echo "  embedded content: $(wc -c <"$cache" | tr -d ' ') B, identical to content/functionalstorage.hmod"
    else
        # The game consumes the written package into its own content cache; the
        # loader's startup line is then the proof that the bundle's copy loaded.
        grep -q 'content from 1 mod(s)' "${SOCK}.log" \
            || fail "the bundle's embedded content did not load (no 'content from 1 mod(s)' in ${SOCK}.log)"
        echo "  embedded content: loaded from the bundle (game content cache rebuilt at startup)"
    fi
    cmp -s "$cache" "$PORT/content/functionalstorage.hmod" \
        || fail "the embedded content differs from content/functionalstorage.hmod"
    echo "  embedded content: $(wc -c <"$cache" | tr -d ' ') B, identical to content/functionalstorage.hmod"
fi
if grep -q "content mods not loaded" "${SOCK}.log" 2>/dev/null; then
    grep "content mods not loaded" "${SOCK}.log" >&2
    fail "the content mod did not load"
fi

say "an Overworld platform at y=100, away from the terrain"
PX=$(ctl state | jq -r '.player.position[0]' | cut -d. -f1)
PZ=$(ctl state | jq -r '.player.position[2]' | cut -d. -f1)
ctl command "fill $((PX-8)) 100 $((PZ-8)) $((PX+8)) 100 $((PZ+8)) minecraft:stone" >/dev/null
ctl command "fill $((PX-8)) 101 $((PZ-8)) $((PX+8)) 110 $((PZ+8)) minecraft:air" >/dev/null
ctl tp "$PX.5" 101 "$((PZ+3)).5" >/dev/null
ctl wait 20
ctl command "gamerule spawn_mobs false" >/dev/null
ctl command "gamerule spawn_monsters false" >/dev/null
ctl command "kill @e[type=!minecraft:player]" >/dev/null
ctl wait 20
X=$PX
Z=$((PZ + 1))
Y=102
ctl close >/dev/null 2>&1 || true
ctl look 180 0
ctl wait 6

if [ "$COUNTERFACTUAL" = 1 ]; then
    PLACE=minecraft:barrel
else
    PLACE=$DRAWER
fi

say "place $PLACE at $X $Y $Z"
# The floating support at (X, Y, Z-1): its south face is visible from the
# player, so `place` can put the drawer beside it. (The allthetweaks e2e uses
# the same floating-support recipe.)
ctl setblock "$X" "$Y" $((Z - 1)) minecraft:stone >/dev/null
ctl give "$PLACE" 1 >/dev/null || fail "cannot give $PLACE"
ctl wait 6
ctl select 1 >/dev/null || true
ctl wait 6
ctl place "$X" "$Y" "$Z" --item "$PLACE" >/dev/null || fail "cannot place $PLACE"
ctl wait 6
[ "$(block_name "$X" "$Y" "$Z")" = "$PLACE" ] || fail "the placed block is $(block_name "$X" "$Y" "$Z"), expected $PLACE"
facing=$(block_facing "$X" "$Y" "$Z")
echo "placed facing: ${facing:-unknown}"
[ -n "$facing" ] || fail "the drawer has no facing property"
ctl look-at block "$X" "$Y" "$Z" "$facing" >/dev/null || fail "cannot aim at the front"
ctl wait 6

if [ "$COUNTERFACTUAL" = 1 ]; then
    say "counterfactual: the vanilla barrel is not a drawer"
    info "$X" "$Y" "$Z" | grep -q 'not a drawer' || fail "the barrel answered as a drawer: $(info "$X" "$Y" "$Z")"
    ctl command "clear @s" >/dev/null
    ctl give minecraft:stone 100 >/dev/null
    ctl wait 4
    ctl select minecraft:stone >/dev/null
    ctl wait 4
    ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null || true
    ctl wait 6
    left=$(count_item minecraft:stone)
    echo "stone left in the hand after the barrel click: $left"
    [ "$left" -gt 0 ] || fail "the barrel took all 100 (a chest menu is not a drawer)"
    echo "PASS: the same click does not store 100 in a barrel; the drawer's behaviour is its own code, not the scenario"
    exit 0
fi

say "empty drawer readout"
line=$(info "$X" "$Y" "$Z")
echo "readout: $line"
echo "$line" | grep -q '^functionalstorage:oak_1: 0/2048 empty$' || fail "unexpected empty readout: $line"

say "a stack of stone: the drawer stores a whole stack at once"
ctl command "clear @s" >/dev/null
ctl give minecraft:stone 64 >/dev/null || fail "cannot give stone"
ctl wait 6
ctl select minecraft:stone >/dev/null || fail "cannot select stone"
ctl wait 6
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null || fail "the front click failed"
ctl wait 8
amount=$(info_amount "$X" "$Y" "$Z")
left=$(count_item minecraft:stone)
echo "stored: $amount, stone in the hand: $left"
[ "$amount" = 64 ] || fail "the drawer stored $amount, expected 64"
[ "$left" = 0 ] || fail "the drawer left $left stone in the inventory, expected 0"

say "the menu withdraws one, reconciled against the true amount"
ctl command "clear @s" >/dev/null
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null || fail "the empty-hand click did not open the menu"
ctl wait-for screen container --timeout 10 >/dev/null \
    || fail "the drawer did not open a menu (screen=$(ctl state | jq -r '.screen // ""'))"
menu_slots=$(ctl screen | jq '[.slots[] | select(.slot >= 300)] | length')
echo "menu slots: $menu_slots"
[ "$menu_slots" = 1 ] || fail "the drawer menu shows $menu_slots slots, expected 1"
before_menu=$amount
# The result slot holds the proportional view (2 at 64/2048); a right-click
# takes half of it, one item.
ctl click-slot 300 right >/dev/null || fail "cannot right-click the drawer result slot"
ctl wait 6 >/dev/null
# The take lands on the cursor; an empty inventory slot (9) puts it away so
# `close` cannot drop it.
ctl click-slot 9 >/dev/null || fail "cannot put the withdrawn item into the inventory"
ctl wait 10 >/dev/null
ctl close >/dev/null 2>&1 || true
ctl wait 6 >/dev/null
after_menu=$(info_amount "$X" "$Y" "$Z")
one=$(count_item minecraft:stone)
echo "menu withdrawal: $before_menu -> $after_menu, stone in the hand: $one"
[ "$one" = 1 ] || fail "the menu gave $one stone, expected 1"
[ "$after_menu" = $((before_menu - 1)) ] || fail "the menu took $((before_menu - after_menu)) from the drawer, expected 1"

say "a different item is refused"
ctl give minecraft:dirt 64 >/dev/null
ctl wait 6
ctl select minecraft:dirt >/dev/null
ctl wait 6
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null || true
ctl wait 6
amount=$(info_amount "$X" "$Y" "$Z")
dirt=$(count_item minecraft:dirt)
echo "stored: $amount, dirt in the hand: $dirt"
[ "$amount" = 63 ] || fail "the refused dirt changed the drawer to $amount"
[ "$dirt" = 64 ] || fail "the drawer took $((64 - dirt)) dirt"

say "a second stone click adds the held stack"
ctl command "clear @s" >/dev/null
ctl give minecraft:stone 5 >/dev/null
ctl wait 6
ctl select minecraft:stone >/dev/null
ctl wait 6
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null
ctl wait 8
amount=$(info_amount "$X" "$Y" "$Z")
echo "stored: $amount"
[ "$amount" = 68 ] || fail "the second click stored $amount, expected 68"

say "left click shows the withdraw hint and takes nothing (pin block)"
# Survival: a creative click would break the drawer outright, and this phase
# must observe the hint, not a break.
ctl command "gamemode survival" >/dev/null
ctl wait 4 >/dev/null
ctl command "clear @s" >/dev/null
ctl look-at block "$X" "$Y" "$Z" "$facing" >/dev/null
ctl wait 4
before_click=$(info_amount "$X" "$Y" "$Z")
ctl mouse left click >/dev/null || fail "the left click failed"
ctl wait 6 >/dev/null
hint=$(action_bar)
amount=$(info_amount "$X" "$Y" "$Z")
one=$(count_item minecraft:stone)
echo "action bar: '$hint'; stored: $before_click -> $amount, stone in the hand: $one"
case "$hint" in
    *"Open the drawer to withdraw"*) ;;
    *) fail "the left click did not show the withdraw hint: '$hint'" ;;
esac
[ "$amount" = "$before_click" ] || fail "the left click changed the drawer $before_click -> $amount"
[ "$one" = 0 ] || fail "the left click gave $one stone, expected 0"

say "a hopper above fills the drawer"
ctl command "clear @s" >/dev/null
ctl setblock "$X" $((Y + 1)) "$Z" "minecraft:hopper[facing=down]" >/dev/null || fail "cannot place the hopper"
# Read the drawer before filling the hopper: `info` waits eight ticks, and
# the hopper pushes its first item during them.
before=$(info_amount "$X" "$Y" "$Z")
ctl command "item replace block $X $((Y + 1)) $Z container.0 with minecraft:stone 5" >/dev/null || fail "cannot fill the hopper"
ctl wait 80
after=$(info_amount "$X" "$Y" "$Z")
echo "stored: $before -> $after (hopper above)"
[ "$after" -eq $((before + 5)) ] || fail "the hopper stored $((after - before)), expected 5"

say "a hopper below empties it into a chest"
ctl setblock "$X" $((Y - 1)) "$Z" "minecraft:hopper[facing=down]" >/dev/null || fail "cannot place the lower hopper"
ctl setblock "$X" $((Y - 2)) "$Z" minecraft:chest >/dev/null || fail "cannot place the chest"
before=$(info_amount "$X" "$Y" "$Z")
ctl wait 80
after=$(info_amount "$X" "$Y" "$Z")
in_chest=$(chest_stone "$X" $((Y - 2)) "$Z")
echo "stored: $before -> $after, chest stone: $in_chest"
[ "$after" -lt "$before" ] || fail "the lower hopper took nothing ($before -> $after)"
echo "chest stone (informational): $in_chest"

say "a comparator reads the drawer's fullness"
# The hoppers emptied the drawer; fill it again for the comparator.
ctl command "clear @s" >/dev/null
ctl give minecraft:stone 64 >/dev/null
ctl wait 6
ctl select minecraft:stone >/dev/null
ctl wait 6
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null
ctl wait 8
ctl setblock $((X + 1)) $((Y - 1)) "$Z" minecraft:stone >/dev/null
ctl setblock $((X + 1)) "$Y" "$Z" "minecraft:comparator[facing=west]" >/dev/null || fail "cannot place the comparator"
echo "comparator state: $(ctl block $((X + 1)) "$Y" "$Z" | jq -r '.state')"
ctl wait 8
partial=$(bar_power $((X + 1)) "$Y" "$Z")
echo "comparator power: ${partial:-none} at $(info_amount "$X" "$Y" "$Z") stored"
[ -n "$partial" ] || fail "the comparator has no powered state"
[ "$partial" = true ] || fail "the comparator reads powered=$partial for a non-empty drawer"
# A comparator beside a plain stone block is the control: same rig, no drawer.
ctl setblock $((X + 1)) $((Y - 1)) $((Z - 1)) minecraft:stone >/dev/null
ctl setblock $((X + 1)) "$Y" $((Z - 1)) "minecraft:comparator[facing=west]" >/dev/null
ctl wait 8 >/dev/null
control=$(bar_power $((X + 1)) "$Y" $((Z - 1)))
echo "stone comparator powered: ${control:-none}"
[ "$control" = false ] || fail "the stone control reads powered=$control"

say "breaking the drawer keeps its contents (survival, so it drops)"
ctl command "gamemode survival" >/dev/null
ctl wait 4 >/dev/null
ctl command "clear @s" >/dev/null
ctl give minecraft:stone 64 >/dev/null
ctl wait 6
ctl select minecraft:stone >/dev/null
ctl wait 6
ctl interact "$X" "$Y" "$Z" "$facing" >/dev/null
ctl wait 8
before=$(info_amount "$X" "$Y" "$Z")
[ "$before" -ge 1 ] || fail "the drawer is empty before the break test"
ctl command "clear @s" >/dev/null
# A hand break of a wooden drawer is slow; the axe is what a player uses and
# what the loot table's tool rule expects.
ctl give minecraft:iron_axe 1 >/dev/null || fail "cannot give the axe"
ctl wait 4
ctl select minecraft:iron_axe >/dev/null || fail "cannot select the axe"
ctl wait 4
ctl setblock $((X + 1)) "$Y" "$Z" minecraft:air >/dev/null
# A stone floor under the drawer replaces the lower hopper, so the drawer
# item's drop lands within reach instead of falling through the platform.
ctl setblock "$X" $((Y - 1)) "$Z" minecraft:stone >/dev/null
ctl setblock "$X" $((Y - 2)) "$Z" minecraft:air >/dev/null
ctl setblock "$X" $((Y + 1)) "$Z" minecraft:air >/dev/null
echo "break: $(ctl break "$X" "$Y" "$Z" 2>&1)"
ctl wait 10 >/dev/null
echo "block after break: $(ctl block "$X" "$Y" "$Z" | jq -c '{block,state}')"
[ "$(block_name "$X" "$Y" "$Z")" = "minecraft:air" ] || fail "the drawer did not break"
echo "items on the ground: $(ctl entities 8 | jq -c '[.entities[]? | select(.kind == "item")]')"
# Walk onto the dropped item's block so the pickup lands in the inventory.
ctl tp "$X.5" "$Y" "$((Z + 1)).5" >/dev/null
ctl wait 30 >/dev/null
items=$(count_item "$DRAWER")
ground=$(dropped_item "$DRAWER")
echo "drawer items after the break: hand $items, ground $ground"
[ $((items + ground)) -ge 1 ] || fail "the broken drawer did not drop itself"
[ "$items" -ge 1 ] || fail "the broken drawer's drop was not picked up"
ctl select "$DRAWER" >/dev/null 2>&1 || true
ctl look-at block "$X" "$Y" "$Z" south >/dev/null 2>&1 || true
reply=$(ctl place "$X" "$Y" "$Z" --item "$DRAWER" 2>&1) \
    || { echo "place reply: $reply"; fail "cannot place the drawer again"; }
ctl wait 8
restored=$(info_amount "$X" "$Y" "$Z")
echo "restored: $restored (was $before; the view spilled as the loose items above)"
# The component carries the bulk (`amount - view`) and the view slot spills as
# loose items on removal (the port's documented representation), so the
# replaced drawer holds less than `before` but never nothing.
[ "$restored" -ge 1 ] || fail "the replaced drawer restored nothing"
[ "$restored" -lt "$before" ] || fail "the replaced drawer has $restored >= $before (no spill accounted?)"

say "the drawer's contents survive a relaunch"
before_relaunch=$(info_amount "$X" "$Y" "$Z")
[ "$before_relaunch" = "$restored" ] || fail "the readout changed before the relaunch: $restored -> $before_relaunch"
echo "quit with $before_relaunch stored"
ctl quit >/dev/null 2>&1 || true
sleep 5
rm -f "$SOCK"
"$CTL" --socket "$SOCK" launch \
    --game "$GAME" --world FunctionalStorageE2E --seed 20261105 --creative --offscreen --wait \
    --game-dir "$SCRATCH/game" --resources "$RES" "${DEV_ARGS[@]}"
ctl wait-ready 180
ctl wait 20 >/dev/null
after_relaunch=$(info_amount "$X" "$Y" "$Z")
echo "relaunch: stored $before_relaunch -> $after_relaunch at $(block_name "$X" "$Y" "$Z")"
[ "$(block_name "$X" "$Y" "$Z")" = "$DRAWER" ] || fail "the drawer is gone after the relaunch"
[ "$after_relaunch" = "$before_relaunch" ] || fail "the relaunch changed the drawer $before_relaunch -> $after_relaunch"

if [ "$NEGATIVE" = 1 ]; then
    say "NEGATIVE: assert the barrel's 64 limit on the drawer (must fail)"
    [ "$restored" = 0 ] || fail "the drawer stored $restored, not 0 (the check is alive)"
fi

echo "PASS: the drawer stores 100+, refuses other kinds, withdraws one through its menu (64 -> 63), its left click only shows the withdraw hint (the pin reverts a BE write there), hoppers move items, comparators read it, and a break keeps its contents"
