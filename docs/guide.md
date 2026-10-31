# Using Glass Cannon

Everything the app does, in detail. For installing it, see the [README](../README.md).

It starts capturing straight away and waits for the game if it isn't running.

## The launcher

Glass Cannon opens to a small **launcher**, laid out like a music player, in its own
colours whatever the theme:

- **the cover**: click it for the analysis of the fight going on, or the last one;
- **the list**: the status (capturing, waiting…) and the game's ping, whether the game is
  running, the overlay, how the fight is going or how it ended, your DPS and share, the
  party's, and the history;
- **the bar**: buttons for the analysis, the overlay's start/stop, the history, the pet
  map, arranging the overlay (again to save and finish) and the settings;
- **the lights** at the top right: minimise, keep on top, and close (which closes the
  app).

Text too long for its line slides along rather than being cut off.

## The overlay

One transparent layer over the game's picture holding the meter and the other widgets, in
your theme. It follows the game window, hides when you switch away, never takes the focus
from the game, and lets the mouse through except over the meter and the world timers.

**Arrange** (on the launcher, the meter, in the settings, or **Ctrl+Shift+F10** in game)
moves widgets and resizes them by their
corner, snapping to a grid (8, 16, 32 or 64 px, or none) shown while arranging. Widgets
that grow with what they show (the target frame, cooldowns, party cooldowns) are outlined
at the most they'll take, so they can be placed clear of each other. Its
floating menu (drag it by its title) turns each widget on or off, resets the layout, and
finishes. The widgets:

- **meter**: the players, below.
- **boss warnings**: a dangerous cast in the middle of the screen, as a card like the other
  alerts: casting, then landing (move), the skill's name, and its time left, with a chime;
  a heads-up 5 s before one is due. Once over, it stays dimmed for 1.5 s. A cast is dangerous when
  it marks the ground or takes 2.5 s or more; in the analysis window's [boss] tab, click a
  skill's alert to make it always or never warn.
- **boss timers**: the boss's next casts from your last fight against it, as bars counting
  down.
- **hidden cubes**: an alert when a hidden cube (gold or red) comes into view, with a
  sound (settings → alert sound); opened cubes show briefly as taken. It can't say which
  way the cube is: the game never sends your own position as you move (and what your game
  sends is encrypted).
- **target frame**: your target's HP, time to kill, what it's casting, enrage, who it's
  attacking, your best against it, stagger. While it shows, the meter leaves out its own
  boss HP, enrage and cast lines, so nothing is shown twice.
- **player bars**: your HP and MP as two thin bars, and a cast bar while you cast. The
  game never sends your max HP: the highest seen stands in (your max once you've been at
  full HP).
- **party frames**: your party as a grid of tiles, each filled by the member's HP in their
  class's colour (known once they've fought), with a thin MP strip, ★ for the leader, and
  "dead" when they are.
- **cooldowns**: your skills that aren't ready, with time left and charges.
- **party cooldowns**: your party's long cooldowns and party buffs, ready or time left
  (yours from the start, the others' once used).
- **world timers**: rifts, invasions, sieges, Arena of Tactics (NA, EU and LA), resets and
  field bosses (open the world map in game to read them) in your time; hover an event for
  its next three starts. Star (☆) the ones you want and **[★ only]** shows just those, and
  ring the 🔔 for an alert on the meter before one starts. Also who holds the
  Abyss artifacts (when you open the Abyss map or enter it; not tested in game yet), your
  Odyle energy, and your content tickets: the game doesn't name them, so double-click one
  to give it a name.

- **pet tracker**: the pets you track (the pet map's **[+]** beside each pet; search one
  there by name, genus or area) with their level, souls toward the next one and every
  area they're found in; a pet whose mobs are in view turns green with how many.
  **[nearest]** shows every pet you still need whose mobs are in view, tracked or not, the
  most first. Which mobs give which souls is from Aion DPS Meter's data; a few mobs aren't
  in it.

**Alt+E** hides the overlay's widgets from in game, and brings them back. That's for now
only: the overlay stays on (and the next start shows them again), the launcher says
they're hidden and which keys bring them back, and its play button brings them back too.
With the overlay off, Alt+E turns it on. Both hotkeys can be changed (settings → keybinds:
click one, then press the keys): a key with Ctrl, Shift, Alt or Win, or an F key. They go
through Windows' hotkeys (as Questlog's overlay does with its own Alt+E), so Windows gets
them before the game does. They're only held while the game is up, and if another app
already has the same keys, the keybinds say so and it's tried again every few seconds.
One modifier alone (a tap of Right Alt, say) works too, read from the keyboard, but the
game gets it as well: Alt shows its cursor.
**Close with the game** (settings → tools) closes Glass Cannon a few seconds after the game
closes. **Reset settings & keybinds** (settings → tools, click twice) puts every setting,
keybind and the appearance back to the defaults; the pets you track, starred and alerted
events, boss skills you marked, the overlay's layout and the history stay.

**Hide on the world map** (settings, on by default): the overlay steps aside while the
world map is open and comes back within about 1.5 s of closing it. The game asks for the map's
contents about every 1.2 s while it's open, and that's how the meter knows.

**Hide for game menus** (settings, on by default): the overlay also steps aside while a
full-screen game menu is open. Menus send the server nothing, the game keeps its keys from
other apps, and the cursor a menu brings up is the same as the one Alt does, so the
cursor alone never hides the widgets. Every full-screen menu (the inventory, the main
menu…) has a close X at the top right, on the corner's diagonal whatever the UI scale (a
few windows, like the Season Mission, a back arrow instead): while the cursor shows, the
meter looks at a small square there (8.5% of the game's height a side) for that X or
arrow, and steps aside only when one is there. Once seen, they stay aside until the cursor
goes (a pop-up over a menu dims its X past seeing; closing the menu hides the cursor). The square is only looked at:
it's never saved or sent, and nothing is read while the cursor is hidden, the setting is
off or the game isn't in front. Keep widgets off that corner (one there hides the X). The
setting's tooltip shows the last checks. With the classic controls, where the cursor
always shows, the corner is simply looked at every half second.

**Fade outside combat** (settings) fades the fight widgets a while after the last fight;
they come back with the next one (the meter under the mouse too). With the **frosted**
background, Windows lightly blurs the game behind each widget.

## The meter

- **auto / mine / party / all**: whose damage counts. *auto* (the default) follows your
  party: your party's fight while you're in a party or expedition, yours on your own.
  *mine* is only your damage, *all* everyone's nearby. In the *party* view (also an
  expedition you're waiting to start), hover a member for their gear, once they've been
  in view: each slot's piece, its enhancement and item level.
- **dmg / heal / taken / party**: damage dealt, healing done (heals over time and
  self-heals), damage taken from mobs, or your party with each member's combat power and
  gear score, and a ✓ for who's readied up in the party finder. A line under the meter says
  who joins, leaves or readies up.
- **Click a player** for their skills. Double-click a skill or the target to rename it.
- A fight ends on the boss's kill (**✓**), when the boss resets (**✗ wipe**), when your
  party leaves combat, on a map change, or after 15 s of quiet; **▷|** ends it now. A live
  boss fight shows the boss's HP and enrage timer; a **☠** marks who died.
- Over the meter, its buttons appear beside the two switches: back (from a saved fight),
  analysis, end the fight, arrange the overlay, history, and **≡** for the settings: theme (*night*, the
  default, or rose, lavender, mint, sky, amber, mono, or any colour), font, background,
  the overlay's widgets, update rate (*live* or every 0.2–2 s), fading outside
  combat, recording and status checks.

## Analysis

**Right-click a player** (or the chart icon, on the meter or the launcher) for the full
fight; right-click again to close it. Tabs:

- **overview**: DPS, crits, positionals and DPS over time with the target's HP.
- **skills**: every skill with the player's chosen specialties and accuracy (hits the
  target evaded, resisted or was immune to are misses); hover for the tooltip.
- **rotation**: the opener and a zoomable cast timeline that follows a live fight.
- **buffs**: buff and debuff uptime with icons, and which buffs were up for each skill.
- **healing** and **taken**: who healed whom (heals on allies, and HP a player's hits
  drain back, included) and what hit whom.
- **deaths**: who died to what, the last hits with HP after each, the healing before, and
  how long until they were revived.
- **boss**: the boss's casts from its pull, with cast times and when their areas landed.

**[copy text]** and **[copy image]** put a summary on the clipboard for Discord.

## History and dungeon runs

The **clock icon** (on the meter or the launcher) opens the history window with the last
200 fights (the newest 50 keep their full analysis), dungeon runs, bosses and PvP sessions;
click a fight to show it on the meter.
Fights in a dungeon are grouped into **runs** with the dungeon, mode, fight time, bosses,
damage and, when the game says it was completed, the clear time (✓) and each stage's
time; your fastest clear and stages of a dungeon and mode get a ★. Right-click a run for
one analysis of all its fights, or a fight for that fight. A run is one entry into the
dungeon (older fights: no break over 10 minutes and no boss fought twice). **[clear]** at
the bottom deletes every fight, dungeon run and PvP session (it asks first); a dungeon
you're in at the time keeps grouping its next fights as a run.

**Personal bests**: your highest DPS and fastest kill against each boss show during the
fight on the meter's boss line, or the target frame while it shows (green while you're
beating it), and as ★ tags on the fights in the history that set them. They're kept per
character, count every kill you were in, and [clear] resets them.

**World PvP**: while you fight players outside your party (as in the Abyss), a line under
the target shows your kills and deaths and the damage dealt and taken; hover it for the
opponents. The history's [pvp] tab keeps each session.

**Dummy mode** (history → [dummy]; on by default): hit a Training Scarecrow and the meter
counts your damage only and times a run (30 s to 5 min, 1 min by default) from your first
hit. A line under the target shows the run's time and your pace against your best clean
run of that length. A run you keep up to the end is saved when the fight ends; one you
stop early isn't. A run is *clean* when no other player buffed you or debuffed the dummy,
so it compares fairly. The [dummy] tab lists your runs (double-click one to label it, with
a gear set or a rotation; right-click to delete it), each label's average, and compares any
two you click: the DPS difference, the difference at the same crit rate (what's left once
crit luck is taken out), and each skill's damage. Give new runs a label with [label].

**Pet souls** (settings → pet souls): a kill that gives a pet's soul shows on the meter with
how many it has toward the pet's next level (5 to summon it, then 25 and 75), e.g. "Red
Spark Ignus soul +1 · 4/25 to level 2". The game sends your pets when you log in, enter or
leave a dungeon and with some other moves, but not every zone change; the meter keeps each
character's from the first time, so it knows them even when started in between.

**Boss casts**: under the boss's HP on the meter (settings → boss casts; the target frame
has them instead while it shows), what it's casting with a bar and the time left, then,
for skills that mark the ground, how long until they land; between casts, what it cast
next the last time you fought it, with a countdown. Times run from the
boss's pull, so they line up from one fight to the next; its routine skills are left out.
The analysis window's **[boss]** tab lists every cast of a fight; most boss skills have no
name in the game data, so double-click one there to name it.

## Windows

The launcher, settings, history, pet map and analysis are windows, with the same lights at
the top right of their title bars (minimise, keep on top, close; each with a small mark). Each opens where you last left it
(the first time beside the launcher, or in game, or with the launcher minimised, in the
middle of the screen), and takes the background chosen in the settings (the settings and
analysis windows stay tinted enough to read).

## Pet map

The launcher's map button opens the **pet map**: each world and Abyss map (Verteron,
Altgard, Eltnen, Morheim, and the three levels of Reshanta) with its zones and their names
(Questlog's map pictures and zones), and a dot wherever a mob that gives pet souls spawns,
in its genus' colour (Fera red, Natura green, Cogni blue, Varian purple, Special gold).
Zoom in for the smaller zones' names. **[+]** beside a pet tracks it for the pet tracker
widget. The list beside it has the
pets on that map (**[still needed]** leaves out the ones you've maxed); a search looks
through every pet, by name, genus or area;
pick a pet, or click a dot, to light up its spawn points and see the areas it's found in
and how far along it is. The pets you track show in green. You show as a ring on the map
you're on, once the meter knows where you are. Scroll to zoom, drag to move, double-click
for the whole map. The spawn points are from Aion DPS Meter's data (MIT); a recording in
Altgard puts the game's mobs on them, 2.4 m off at the median. Only the map showing is
kept in memory, drawn as Questlog draws its own: in levels of detail, a piece at a time.

## Names

Players are named when they come into view, after a zone change, or when they start a
fight; until then they show as `#ID`. Mobs and NPCs are named from the game data; the few
it doesn't have (and mobs already there when the meter started) show as **Unknown**, with
their type in the tooltip: double-click the target on the meter to name one. You're
recognised on your first hit on a real mob.
