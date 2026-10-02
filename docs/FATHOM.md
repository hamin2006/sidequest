# FATHOM — design document

> *The deeper you go, the less you can see. Every time you look, something hears you.*

FATHOM is a sonar roguelike RPG for the terminal. You pilot a two-person research submersible down an
ocean trench in total darkness. The only way to see is to **ping**: a sonar pulse sweeps outward,
the walls and creatures it touches flare up as echoes, and then they fade back into black.

Every ping is also a shout. Things in the deep hunt by sound.

## The one idea

**Information costs danger.** That single tension drives every decision:

- Ping to see the cave ahead, and the eel two caverns over now knows exactly where you are.
- Stay silent, listen on the hydrophone, and creep forward through a map you half-remember.
- Turn your lamp on and you can see what's right in front of you, and so can everything that hunts by light.
- Echoes show you where a creature **was**. The ghost of an eel fading on your screen is a picture of
  the past. The eel itself has already moved.

Wayfarer was about bright open worlds, combat and loot. FATHOM is the opposite: darkness, stealth,
evasion and knowledge. You rarely fight. You survive by understanding.

## Why it fits "while Claude is thinking"

- **Instant pause.** Time only moves while you play. The game freezes the moment Claude finishes, and
  resumes later with a 3-second countdown so you never come back to an instant death.
- **Bite-sized dives.** A dive is a few minutes. You can split one across several Claude waits; it
  autosaves continuously.
- **Long arc.** Upgrades, a bestiary and a mystery give you a reason to keep coming back over weeks.

## The loop

```
   ┌──────────── RV TERN (surface hub) ────────────┐
   │ workshop · research · logbook · choose drop    │
   └───────────────┬────────────────────────────────┘
                   ▼
          DIVE: descend, chart, scan, salvage
                   │
       ┌───────────┴───────────┐
   surface with cargo      hull fails
   (bank everything)       (lose cargo; keep knowledge)
       └───────────┬───────────┘
                   ▼
            upgrade, go deeper
```

- **Cargo** (scrap, relics) only counts once you bring it back up. Dying loses it.
- **Knowledge** (bestiary scans, expedition logs, charted zones) is transmitted instantly and is never lost.
- Greed versus safety: one more wreck, or turn back while you still have battery?

## The trench

A side-on cross-section, 96 cells wide and 1,100 rows deep. One row is 10 metres, so the floor sits at
about 10,900 m, the depth of the real Challenger Deep. Every dive generates a new trench from a seed: a
guaranteed meandering main channel plus noise-carved side caverns that narrow and twist with depth.

| Zone | Depth | What lives there |
| --- | --- | --- |
| Sunlit | 0–200 m | the ship above, calm water, the tutorial |
| Twilight | 200–1,000 m | glimmer shoals, drift jellies |
| Midnight | 1,000–4,000 m | gulpers, anglers, the first hunter eels |
| Abyssal | 4,000–6,000 m | eel packs, hydrothermal vents, rich wrecks |
| Hadal | 6,000–10,800 m | the leviathan, the Song |
| The Floor | ~10,900 m | the Meridian, and whatever is singing |

Crossing into a new zone drops a relay buoy. Later dives can be lowered straight to any relay you've
reached, so you never have to re-swim the shallows.

## Seeing in the dark

| Sense | Cost | What it shows |
| --- | --- | --- |
| **Lamp** (always on unless switched off) | small battery drain; light-hunters can see you | a tiny radius around the sub, live |
| **Sonar ping** | battery; loud noise in a wide radius | everything in line of sight out to range, as fading echoes |
| **Hydrophone** (passive, free) | nothing | arrows on the screen edge pointing at things that are making noise |
| **The chart** | nothing | walls you've already pinged stay drawn faintly for the rest of the dive |

Echoes fade over about seven seconds. A creature's echo is a snapshot: a ghost glyph frozen where it was
when the ring passed.

Sonar respects rock. Things behind walls stay hidden, so you sometimes have to move to see.

## Things in the deep

| Glyph | Species | Behaviour | Threat |
| --- | --- | --- | --- |
| `·` | Glimmer shoal | drifts, scatters from noise, blinks faintly | harmless, easy research |
| `Ω` | Drift jelly | rides currents; flares bright when pinged | sting + drains battery |
| `Θ` | Gulper | motionless ambusher in side caves | huge bite if you pass next to it |
| `Ψ` | Angler | its lure `◦` glows like a wreck beacon | lures you in, then bites |
| `ξ` | Hunter eel | hunts sound: swims to the last noise it heard | fast, repeated bites |
| `█` | Leviathan | sleeps in the Hadal zone; noise fills its "disturbance" | wakes and chases; near-fatal |
| `*` | The Singer | the end of the trench | — |

**Scanning:** each time a ping's echo touches a creature, you gain research on its species. Three
echoes complete a bestiary entry, with notes and a gameplay hint, and award research points.
Scanning dangerous things means pinging near dangerous things.

## Tools

| Key | Tool | Notes |
| --- | --- | --- |
| `space` | Ping | battery cost; noise radius ≈ 1.3 × sonar range |
| `f` | Lamp on/off | dark is quieter but blind |
| `q` | Decoy | drop a noisemaker that hunters chase for 8 s (limited charges) |
| `e` | Salvage | when touching a wreck hatch; takes 2 s, you're stuck while it runs |
| `h` | Harpoon | unlocked by upgrade; kills small creatures, noisy |
| `r` | Ascend | autopilot back to the surface along charted water, interrupted by any key |
| `tab` | Chart | zoomed-out map of everything charted this dive |

## The sub (RPG progression)

**Scrap** buys workshop upgrades. **Research points** (from scans) buy techniques.

| Workshop | Effect per level |
| --- | --- |
| Pressure hull | deeper safe depth: 1,500 → 3,000 → 5,000 → 7,500 → 11,000 m |
| Battery | +40 capacity |
| Sonar array | +5 range |
| Quiet drive | −25% engine and ping noise |
| Hull plating | +25 max hull |
| Decoy rack | +1 decoy |
| Harpoon | unlocks, then +damage |

| Research | Effect |
| --- | --- |
| Echo memory | echoes linger 50% longer |
| Lure filter | angler lures show red instead of amber |
| Pressure model | below rated depth, hull damage halves |
| Pulse compression | pings cost 30% less battery |
| Song analysis | the hydrophone also hears the leviathan's breathing |

Below the hull's rated depth the hull creaks and takes pressure damage, faster the further you go.
That gates progress naturally, without walls.

**Rank:** depth and discoveries earn XP. Rank titles go from Cadet → Pilot → Deep Pilot → Abyssal
Cartographer → Keeper of the Floor.

## The mystery: the Meridian

In 1994 the research sub *Meridian* went into this trench with three people aboard and never came back:

- **Dr. Ilse Varga**, acoustician
- **Tomas Reyes**, pilot
- **Ana Okafor**, biologist

Twelve log fragments lie in wrecks at increasing depth, and pieced together they tell the story. The
crew started hearing a structured signal from the floor. It had rhythm and repetition, almost a song.
Then their sonar began returning echoes that didn't match the terrain. Varga worked it out: something
down there was answering their pings, using their own ping as its voice.

At the floor you find the *Meridian*, and your ping comes back in her signature. Two endings:

- **Answer the Song.** Ping in its rhythm. The Singer guides you to the Meridian's last recording.
- **Go dark.** Kill the lamp, stay silent, recover the black box and climb. Something follows you up.

Both endings unlock a New Game+ "Second Descent" with a changed trench.

## Feel

- Black screen, cyan echoes, amber beacons, red danger. Slow fades. Silence punctuated by the ring.
- Messages are terse, like a sonar operator's: *"Contact. Bearing 040, range 30m."*
- The hydrophone arrows and a hull creak at depth do more for tension than any music.

## Scope for v1

The full loop, all zones, six creature types plus the ending, the workshop and research, 12 logs, the
bestiary, relays and both endings. Saves after every action. Not in v1: crew members, multiple subs,
procedural events beyond creatures and wrecks.
