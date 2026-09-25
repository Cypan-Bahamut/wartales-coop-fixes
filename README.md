# Wartales Co-op Fixes

Community bytecode patches for two long-standing Wartales co-op bugs. You run a
small tool against **your own** `hlboot.dat` — no game files are distributed
here, nothing in your install is modified beyond that one file, and Steam's
"Verify integrity of game files" undoes everything.

Unofficial. Not affiliated with Shiro Games. Use at your own risk (which is
low: one file, backed up, fully reversible).

## The fixes

### 1. Loot-screen flicker in co-op (`lootfix2`) — **tested, confirmed working**

The bug: after a co-op battle, the loot/debrief screen flickers, the cursor
flips between two sprites, and you have to spam-click to grab anything.
Reported on the official tracker as
[coop looting](https://wartales.featureupvote.com/suggestions/614993/coop-looting) and
[Co-op End of fight loot bug](https://wartales.featureupvote.com/suggestions/740453/coop-end-of-fight-loot-bug).

Root cause (from disassembly of the game's HashLink bytecode): in multiplayer
only, the debrief window's per-frame update re-checks "can this player afford
the repairs / cure all injuries?" and **rebuilds the entire window** whenever
that answer disagrees with the button state it was built with. If you can't
afford the repair bill or lack remedies for your injuries, the disagreement is
permanent and the window tears itself down and rebuilds **every frame** — that
is the flicker, and why clicks only land if you hit the right frame.

This also explains the community folklore: it "travels with the save" (a save
where you're chronically short on krowns/remedies flickers every battle), and
reloading the fight sometimes fixes it (different damage rolls change whether
you're under the affordability line). A no-patch workaround follows directly:
**keep krowns and remedies stocked above your repair/cure needs** and the loop
cannot start.

The patch changes one byte: multiplayer takes the same early exit from that
per-frame check as singleplayer. Known cosmetic trade-off: the Repair/Cure
buttons no longer live-update their enabled look while the screen is open;
worst case a button looks clickable and does nothing.

### 2. Untargetable "Nightmare" reinforcements in ghost-pack fights (`nightmarefix`) — **beta, in field testing**

The bug: in the fog battles against ghost animals, the Nightmare that arrives
as reinforcements can be attacked by the host but **not by the co-op guest** —
the guest sees it, it fights normally, but it can't be clicked and the
movement preview paths straight through it. Normal-battle reinforcements are
unaffected.

Root cause: in fog battles the game computes unit visibility per client, but
guests are forbidden from recomputing it (`tryUpdateVisibility` exits unless
you are the host) — they depend on one-shot reveal notifications from the
host. If that notification races the guest's replica of a walking-in
reinforcement, the guest's client keeps `visible = false` forever, and both
click-targeting and pathfinding skip invisible units. Nothing on the guest
ever retries. (This is why only fog battles are affected: normal battles don't
run visibility checks at all.)

The patch (a) allows any client to recompute visibility locally — the fog math
itself is untouched, so genuinely hidden units stay hidden — and (b) refreshes
each candidate unit's visibility whenever a skill gathers its targets, which
runs on the guest's machine every time they select an attack. Self-healing: a
lost notification is repaired at the next attack selection.

## Install

Requirements: Wartales on PC. The tool anchors everything by name and refuses
with a clear message if your game version doesn't match what it expects.

1. Close the game. In your install folder (e.g.
   `C:\Program Files (x86)\Steam\steamapps\common\Wartales`), back up
   `hlboot.dat` (copy it to `hlboot.dat.bak`).
2. Download `patchtool.exe` from the Releases page (or build from source,
   below) into that folder.
3. Apply the tested loot fix:

   ```
   patchtool.exe lootfix2 hlboot.dat hlboot_patched.dat
   ```

   Optionally add the beta nightmare fix on top:

   ```
   patchtool.exe nightmarefix hlboot_patched.dat hlboot_patched2.dat
   ```

4. Replace `hlboot.dat` with the patched output (rename the output file to
   `hlboot.dat`).
5. In co-op, **install on all machines** (the loot fix matters on whichever
   machine flickers; identical files avoid surprises).

Check what a file contains at any time:

```
patchtool.exe verify hlboot.dat
```

**Uninstall / rollback:** restore `hlboot.dat.bak`, or Steam → Properties →
Installed Files → Verify integrity. Note that any game update silently
restores the original file — if a bug comes back after a patch day, just
re-run the tool (it either applies cleanly to the new version or refuses and
tells you why).

## Build from source

Rust (any recent stable). The tool builds against the byte-identical hlbc
fork vendored in [wartales-mp](https://github.com/UberMorgott/Wartales-Mod-wartales-mp)
(see `Cargo.toml` for the path setup):

```
cargo build --release
```

## Credits

- [hlbc](https://github.com/Gui-Yom/hlbc) by Guillaume Anthouard (MIT) — the
  HashLink bytecode library this stands on.
- [wartales-mp](https://github.com/UberMorgott/Wartales-Mod-wartales-mp) by
  Morgott (CC BY-NC 4.0) — the vendored hlbc round-trip fixes and the prior
  art for safe Wartales bytecode patching.
- Diagnosis and patches: Cypan, with AI assistance (Claude).

License for this repository's own code: MIT. Wartales is a trademark of
Shiro Games; this is an unofficial community fix and distributes no game
content.
