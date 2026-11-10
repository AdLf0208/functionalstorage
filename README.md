# functionalstorage (Horizon port)

A Horizon port of [functionalstorage](Buuz135/FunctionalStorage) — MIT.

**Status: first slices.** What is below is what the port actually does today.

## Implemented so far

- Ten wood-coloured 1x1 drawers (`functionalstorage:<wood>_1`), one big slot
- each, one item kind per drawer, capacity 2048 for a 64-stack item (upstream
- `BigInventoryHandler`: `64 * base`, base 32 for `X_1`, scaled by the item's
- stack size). The carried content is the upstream models/textures (MIT), the
- recipes, loot tables, lang, one creative tab and the drawer menu type.
- | Area | Upstream | This port |
- |---|---|---|
- | capacity | `BigInventoryHandler.getTotalAmount = 64 * base`, `getSlotLimit = floor(total * maxStack / 64)`; `X_1` base 32 → 2048 (512 for a 16-stack item, 32 for a 1-stack item) | same, `runtime::capacity` |

## Not yet ported

- The left-click **take** stays removed by the campaign pin ruling: a left click
- on the front only shows the action-bar hint "Open the drawer to withdraw" and
- takes nothing; withdrawal is the one-slot menu's result-slot take (verified
- → 63). The pin still reverts a block-entity write from `left_click_block`
- (and the other non-player-action contexts), so the take was not re-enabled.
- The other documented gaps (no front count text, no upgrades, no 1x2/2x2 or
- other families, the locked-drawer hopper quirk, wall-clock cooldowns) are
- unchanged.

## Install

Build the bundle with a native-mod capable launcher, or add the file from the
registry once it is listed. Pinned to Horizon `1d63e0a` and horizon-mixins
`5a64af93` for game 26.3.

## Build from source

```sh
cargo test
python3 tools/horizon.py check --horizon <horizon checkout>
python3 tools/horizon.py build --horizon <horizon checkout>
python3 tools/package.py
```

## Credits and licence

Original mod: Buuz135/FunctionalStorage. Port: AdLf0208. Licence: MIT — see [LICENSE](LICENSE);
the original notices are preserved.
