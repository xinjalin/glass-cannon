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
corner, snapping to a grid (8, 16, 32 or 64 px, or none) shown while arranging. Faint
dashed lines mark the screen's centre: a widget dragged near it snaps to it, and the line
lights up; its edges and centre also snap to another widget's edges or centre, with a
line showing the line-up. Widgets
that grow with what they show (the cooldowns) are outlined at the most they'll take, so
they can be placed clear of each other; the target frame (stagger gauge included) and the
party cooldowns (the whole party) are always as tall as what they show (only their width
resizes), moving up rather than off the screen. Its
floating menu (drag it by its title) turns each widget on or off, resets the layout, and
finishes. The widgets:

- **meter**: the players, below.
- **boss warnings**: a dangerous cast in the middle of the screen, as a card like the other
  alerts: casting, then landing (move), the skill's name, and its time left, with a chime;
  a heads-up 5 s before one is due. Once over, it stays dimmed for 1.5 s. A cast is dangerous when
  it marks the ground or takes 2.5 s or more; in the analysis window's [boss] tab, click a
  skill's alert to make it always or never warn. Dungeon bosses' **mechanics** come first
  (settings → boss mechanics): what it is and what to do, with its time left when known.
  They go off on a buff on the boss (Lakshmi's reflect: stop attacking), a debuff on you
  (a bomb or pool to carry away from the party, then "out of the fire") or on an ally (who
  to free, with their name), the boss's cue casts, its stagger check (while the gauge can
  be broken), and a pool you're standing in: 34 mechanics of 18 bosses in 9 dungeons and
  raids (`gamedata/boss-mechanics.json`). A few casts that killed the most in our runs
  always warn, though short: in the Ludra raid, Rahu's, Ketu's, the Fusion Reactor's and
  five of Eternal Ludra's (her casts are 1.5 to 2.3 s).
  **What a move is called**: the game's own name for it when it has one (Ludra's Trial,
  Abyssal Gaze, Wheel of Torment). The game calls most boss moves only "Attack"; for the
  deadliest of those in the Ludra raid the card shows our own short label for what the
  move does, from our runs: *raid-wide blast* (it hits most of the raid), *area blast*,
  *strike* (one player), *heavy* (half a health bar or more), *pulses* (again and again),
  *marked ground* (the game marks where it lands). Any other unnamed move shows the boss's
  name. Name one yourself in the analysis window's [boss] tab and yours is used instead.
  When the boss you're fighting turns to attack you, the card says so for 3 s (its name
  over "on you", in amber; its mechanics and casts go first), with a lower chime (settings → boss
  on you). It counts once the boss has stayed on you for 0.3 s, since a boss can point at
  someone and move on at once; the target frame keeps showing "→ you" while it lasts.
  A boss that was already up when the meter started (a raid boss stays up through every
  wipe) shows as **Boss**: the meter never saw its spawn, so it can't name it, but it
  knows it's a boss by its stagger gauge, and its HP, wipes, casts and warnings work.
- **boss timers**: the boss's next casts from your last fight against it, as bars counting
  down.
- **hidden cubes**: an alert when a hidden cube (gold or red) comes into view, with a
  sound (settings → alert sound); opened cubes show briefly as taken. It can't say which
  way the cube is: the game never sends your own position as you move (and what your game
  sends is encrypted).
- **target frame**: your target's HP, time to kill, what it's casting, enrage, who it's
  attacking, your best against it, stagger. While it shows, the meter leaves out its own
  boss HP, enrage and cast lines, so nothing is shown twice. A player you target shows
  their class (icon and name, in its colour), their rank (as the game sends it, 1 the
  highest and 19 the lowest) and their combat power, from when they came into view.
- **player bars**: your HP and MP as two thin bars, and a cast bar while you cast. Your
  max HP is the game's (it moves with buffs); until it sends it, the highest HP seen
  stands in. A shield on you (a barrier, such as a Cleric's) shows striped white on from
  your HP's end, with its amount after your HP (+1,215); when HP and shield together are
  more than the bar holds, it covers the bar's last part. Shields show on the vitals arcs
  and the party frames the same way.
- **vitals arcs**: the same, round, like GT Companion's center HUD: arcs either side of
  your character with no box, on the left HP with your stamina inside it, on the right
  MP with your flight power inside it, each filling from the bottom with its value by its
  top end, and your cast on a small arc under your feet. It starts in the middle of the
  screen; arrange it around your character (wider than tall), and the arcs take its box.
  HP glows below 30%. Stamina and flight power show full until they change (the game
  sends them only then). Its look is in the settings (overlay → vitals arcs): **solid**,
  **thin** (glowing lines), **segmented** (cells), **gradient** (dark at the bottom, bright
  where the fill ends), **framed** (a light frame like glass, the fill inside it) or **ticks**
  (thin, with notches outside them like a ruler's, at a quarter, half and three quarters) arcs; a **damage trail**, where HP
  you lose stays as a pale strip for a moment before shrinking away; each arc's colour (HP, stamina, MP,
  flight power, cast, and the shield over HP), picked from a swatch; how round they are, from straight to
  very round; how wide; **always show arcs**, which keeps their tracks up with nothing to
  fill them; the numbers (kept clear of the arcs, and never resizing them) as value and
  max, percent, or both; their size; what's behind
  them (an outline, a shadow, a dark plate or nothing); whether they go by the arcs' top
  or bottom ends; and the cast (on or off) as an arc or a thin straight bar with its time
  left and whole time at its ends.
- **party frames**: your party as a grid of tiles, each filled by the member's HP in their
  class's colour (known once they've fought), with a thin MP strip, ★ for the leader, and
  "dead" when they are. Shields show striped white past their HP, with the amount in the
  tile's tooltip.
- **cooldowns**: your skills that aren't ready, with time left and charges: as a **list**,
  or as a **timeline** (settings → overlay → cooldowns), a bar from Ready to 100% with each
  skill's icon on it as far from Ready as the share of its cooldown left, sliding to Ready
  as it runs down, its time left on it, and a flash when it's ready. Ready can be on the
  left or the right, its size (70–250%: its icons, times and labels), and its scale: by
  **percent** of each cooldown left, or by **time** left, marked 1, 10, 30, 1m, 2m, 5m
  (the last seconds get the most room; longer cooldowns wait at the end). Icons that
  overlap take turns going see-through, so those under them show; clear of the others,
  they're solid.
- **action bar**: the skills you choose, each with its cooldown sweeping round its icon
  and the time left, its charges, and a glow when it's ready again; then the buffs on you
  that you choose to follow, with their time left (and how many, when more than one is on
  you). Choose them in the settings (overlay → action bar): click a skill or buff to put it
  on the bar or take it off; they go on in the order you add them. Its look: square icons,
  round icons, or a bar each with its name and time. The bars' colours are yours to pick:
  a ready skill's (full, green at first), a running buff's (blue) and a skill on
  cooldown's (red), each a colour of the list, the bar colour, or any colour; and the
  faint track behind them can be turned off. Each class has its own bar, picked
  by the class you're playing. The skills to choose from are your class's, from the list
  the game sends at login and the ones you've used; the buffs, those on you in the last
  hour.
- **party cooldowns**: your party's long cooldowns and party buffs, ready or time left
  (yours from the start, the others' once used). Settings' **party cooldowns by** lists
  them by player, or by class: each class's skills with who has them, ready first.
- **world timers**: rifts, invasions, sieges, Arena of Tactics (NA, EU and LA), resets and
  field bosses (open the world map in game to read them) in your time; hover an event for
  its next three starts. Star (☆) the ones you want and **[★ only]** shows just those, and
  ring the 🔔 for an alert on the meter before one starts. The region whose schedule it
  follows and how long before the alert shows (1 to 15 minutes) are in the settings'
  alerts tab (world timers); the widget's top line says which. Also who holds the
  Abyss artifacts (when you open the Abyss map or enter it; not tested in game yet), your
  Odyle energy, and your content tickets: the game doesn't name them, so double-click one
  to give it a name.

- **pet tracker**: the pets you track (the map's **[+]** beside each pet; search one
  there by name, genus or area) with their level, souls toward the next one and every
  area they're found in; a pet whose mobs are in view turns green with how many.
  **[nearest]** shows every pet you still need whose mobs are in view, tracked or not, the
  most first. Right-click a tracked pet to stop tracking it; a tracked pet that reaches max
  level stops being tracked by itself. Which mobs give which souls is from Aion DPS Meter's data; a few mobs aren't
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
**Close with the game** (settings → general) closes Glass Cannon a few seconds after the game
closes. **Reset settings & keybinds** (settings → general, click twice) puts every setting,
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
arrow, and steps aside only when one is there. Reward screens (a Monster Cube's after
content, Shugo's…) have no X there but a gold **Confirm** button at the bottom middle: the
meter also looks at a strip there (30% of the game's height wide, its lowest 22%) for
it. Windows that aren't full screen (Attendance…) have their close X at their own top
right, on their title bar: the meter also looks over the top of the screen (its top 60%,
from 30% across) for an X on a title bar. Once seen, they stay aside until the cursor goes (a pop-up over a menu dims its X past
seeing; closing the menu hides the cursor). The square and the strip are only looked at:
never saved or sent, and nothing is read while the cursor is hidden, the setting is off
or the game isn't in front. Keep widgets off the corner and the button (one there hides
them; the meter leaves its own widgets out of what it looks at, so one there is never
taken for a menu). The setting's tooltip shows the last checks. With the classic controls,
where the cursor always shows, they're simply looked at every half second.

**Click-through** (settings → overlay, off by default): every click goes through the
widgets to the game, the meter's, the world timers' and the pet tracker's too. The other
widgets only show things and always let clicks through. Arranging the widgets still takes
the mouse, so they can be moved and resized.

**Fade outside combat** (settings) fades widgets a while after the last fight; they come
back with the next one (the meter under the mouse too). Which ones fade is up to you: the
arrange menu has a column for it, **[fades]** or **[stays]** for each widget (at first the
meter, boss timers, target frame, cooldowns and party cooldowns fade). With the **frosted**
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
  who joins, leaves or readies up. In a party-finder lobby (before the group starts), when
  a game menu opens and the other widgets step aside, the meter stays and shows the party,
  so everyone's combat power and gear score can be checked from the lobby screen. The
  party's order: as the game lists it, or by combat power or gear score, highest or lowest
  first (settings → meter → party order, or click the cp or gs column to go through them).
  A Sanctuary's room (Abyssal Forge: Ludra, ten players) shows as its two parties, as the
  game's room list has them: each with how full it is, its average combat power and who's
  ready, sorted within itself. Everyone's class comes from the room's list, before they fight.
- The meter's height **fits what it shows** (settings → meter → fit height, on at first):
  its rows, open skill lists and the lines over them, up to 70% of the screen. Off, it's
  the height you give it while arranging the overlay.
- Each player's **combat power** shows by their name (settings → meter → combat power):
  your party's from its roster, anyone else's from when they came into view, yours from
  your login or zone change. **Odyle energy** (settings → meter; charged in brackets) shows
  on the strip at the meter's top, as the game last sent it (at login and as it changes).
- **Text** (settings → appearance → text), as subtitles have it: the font and its size, the
  text's colour (the theme's or any) and opacity, and its edge (the theme's soft shadow,
  none, a drop shadow, an outline, raised or depressed) in a colour of your own. It shows
  everywhere: the overlay and the windows. What's behind the text is the background and
  tint (settings → appearance → background).
- **Class colours** (settings → appearance): with the bars in class colours, each class's
  colour can be your own (click its swatch); the text on it stays readable, dark or
  light by the colour. They show wherever class colours do; **[reset]** brings back the
  built-in ones.
- The rows show each player's share as a strip of **blocks**, or as **bars**: the row
  itself filled that far (settings → appearance → rows). **See-through highlight** makes
  the filled rows as see-through as the tint.
- **Click a player** for their skills. Double-click a skill or the target to rename it.
- A fight ends on the boss's kill (**✓**), when the boss resets (**✗ wipe**), when your
  party leaves combat, on a map change, or after 15 s of quiet; **▷|** ends it now. A live
  boss fight shows the boss's HP and enrage timer; a **☠** marks who died.
- Over the meter, its buttons appear beside the two switches: back (from a saved fight),
  analysis, end the fight, arrange the overlay, history, and **≡** for the settings, in tabs: **appearance** (theme:
  *night*, the default, or rose, lavender, mint, sky, amber, mono, or any colour; bars,
  font, background), **overlay** (its widgets, hiding it, fading outside combat),
  **alerts** (boss warnings and their sound), **meter** (whose damage counts, update rate:
  *live* or every 0.2–2 s, what shows on it, recording), **keybinds** and **general**
  (status checks, closing with the game, resets; a dot on it when a check needs a look).
  The search over the tabs (**Ctrl+F** or **/**) is forgiving: letters in order find a
  setting ("alrt snd": alert sound), a slip is fine ("soudn"), and common words find
  theirs ("opacity": tint, "fps": update rate). **Enter** goes to the best match, **Esc**
  clears the search (opening that setting's tab), and **Ctrl+Tab** steps through the tabs.

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
200 boss fights and the last 200 other fights (the newest 50 of each keep their full
analysis, so short fights don't push out a raid's pulls), dungeon runs, bosses and PvP
sessions; click a fight to show it on the meter.
Fights in a dungeon are grouped into **runs** with the dungeon, mode, fight time, bosses,
damage and, when the game says it was completed, the clear time (✓) and each stage's
time; your fastest clear and stages of a dungeon and mode get a ★. Right-click a run for
one analysis of all its fights, or a fight for that fight. A run is one entry into the
dungeon (older fights: no break over 10 minutes and no boss fought twice). **[clear]** at
the bottom deletes every fight, dungeon run and PvP session (it asks first); a dungeon
you're in at the time keeps grouping its next fights as a run.

Fights are saved as they end, whether the overlay is showing or not. Closing the app (or
the game, with **close with the game** on) ends the fight going on and saves it too.

**Personal bests**: your highest DPS and fastest kill against each boss show during the
fight on the meter's boss line, or the target frame while it shows (green while you're
beating it), and as ★ tags on the fights in the history that set them. They're kept per
character, count every kill you were in, and [clear] resets them.

**Raids** ([raids] in the history): the Sanctuaries (Abyssal Forge: Ludra and the others),
their runs and their fights. Every boss wipe says how far it got, as the boss's lowest HP
("✗ wipe at 72.9%"), and the wipe that got a boss lowest is your **★ best try** against it
(per character).

**Attempts** (the flag icon on the launcher, or **[attempts]** at the bottom of the
history): a window with each character's tries at every boss, a tab per character. Each
dungeon or raid, in each mode, lists its bosses in the order you first tried them with
their tries, kills (✓) and wipes (✗), and the best of them: your fastest kill, or until
the first kill the lowest the boss got. The tabs narrow it: **[raids]** (the
Sanctuaries), **[dungeons]** (Expeditions and Transcendence), **[sealed & strongholds]**
(each Sealed Dungeon and Stronghold, cleared once, with its boss) and **[field & other]**
(field bosses, the three Dimensional Invasions, and anything else). It counts every boss
kill and wipe from when it was added (starting with the fights already in the history),
and the history's [clear] doesn't touch it: its own [clear] forgets one character's tries
(it asks first). A boss the meter never saw spawn has no name to file it under, so it
isn't counted.

**Black market** (the shop icon on the launcher): what the Black Cloud Merchant offers each
character this hour. Open it in the game's shop (H), on each of its pages (Special, Pets,
Skins), with the meter open: each page's offers show here, named, with their price and
discount, and whether they're bought. Pets you don't have yet (as the game last sent your
pets) are marked; skins can't be yet (the game's list of your skins uses other numbers
than the shop's). Each offer says how often you've seen it on offer: each page of each of
your characters counts once an hour, so it's only what you've seen (the app never goes
online). **[worth a look]** shows just the pets you don't have and what you've rarely
seen. The offers change on the hour, and differ per character: a page from before the
hour turned is dimmed until you open it again. When you open the merchant in game, this
window opens by itself (without taking the focus from the game), at most once every two
minutes; turn that off in the settings (general → **open the black market with the
game's**).

**Checklist** (the checklist icon on the launcher): each character's Odyle energy (basic
and charged) and its weekly sources, with how much of each is done this week and when
the week starts again (Wednesday 07:00 UTC, 16:00 in Korea). The weekly Kina buy of Odyle
Energy at the trade guild (20 a week) comes from the game: log in, or open a shop, with
the meter open. The two weekly conversions into Odyle Energy (16 and 4 a week) aren't in
anything the game sends that we've found, so tick them with **[+]** and **[−]**. Each
character keeps its own, and all of it starts again at the weekly reset.

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

## When the meter shows no fights

The launcher's status line says how far the meter got: waiting for the game, waiting for
it to connect, capturing on an adapter (named), or an error. Settings → general shows
whether Npcap and the game data loaded.

**Packets dropped**: the meter's bottom line says so when the capture missed packets (the
PC was busy), with how many: the fight's numbers may be a little low. It goes a minute after
the last drop.

**Ping reducers and VPNs** (ExitLag, GearUP, WTFast, NoPing…): these reroute the game's
connection, often through a relay on your own PC, which passes it on in a tunnel of its
own. The meter follows the game's connection wherever it goes: to the relay, it reads it
off your PC's loopback traffic (where the game's messages still are); to a VPN's adapter, on
that adapter. Reading loopback needs Npcap's "Support loopback traffic" (ticked by
default): if it was unticked, the launcher says so; reinstall Npcap with it ticked. If the
game's connection moves while you play (a ping reducer turned on or off), the capture
starts again on the right adapter within a few seconds.

**Npcap for administrators only**: with "Restrict Npcap driver's access to Administrators
only" ticked when it was installed, the meter (which runs without admin rights) can't
capture. Reinstall Npcap with it unticked.

## Windows

The launcher, settings, history, attempts, checklist, black market, map and analysis are windows, with the same lights at
the top right of their title bars (keep on top, and close; each with a small mark). The
launcher also has minimise; the other windows don't, since they have no taskbar button to
come back from: close one, and its button brings it back. Each opens where you last left it
(the first time beside the launcher, or in game, or with the launcher minimised, in the
middle of the screen). Like the launcher, the windows have a solid background of their own
whatever the theme, tint or background chosen (those are for the overlay's widgets over
the game); the theme gives their accents.

## Map

The launcher's map button opens the **map**: each world and Abyss map (Verteron, Altgard,
Eltnen, Morheim, and the three levels of Reshanta) with its zones and their names
(Questlog's map pictures and zones). **[pets]** and **[gathering]** at the top of its list
switch what it shows; it remembers the one you were on.

**Pets** puts a dot wherever a mob that gives pet souls spawns, in its genus' colour (Fera red, Natura green, Cogni blue, Varian purple, Special gold).
Zoom in for the smaller zones' names. **[+]** beside a pet tracks it for the pet tracker
widget (not one you've maxed). The list beside it has the
pets on that map, and the filters under the search choose which, on the list and the map:
**[all]**, **[missing]** (pets you have no copies of: never summoned), **[needed]** (the
default: not summoned or below max level) or **[tracked]**, and each genus on or off
(click Fera, Natura, Cogni, Varian or Special). Missing and needed go by your pets as the
game last sent them (at login, changing character or entering a dungeon); a search looks
through every pet, by name, genus or area;
pick a pet, or click a dot, to light up its spawn points and see the areas it's found in
and how far along it is. Picking a pet in the list moves the map to its spawns, switching
to the map with the most of them when it isn't on the one showing. The pets you track show in green. You show as a ring on the map
you're on, once the meter knows where you are. Scroll to zoom, drag to move, double-click
for the whole map. The spawn points are from Aion DPS Meter's data (MIT); a recording in
Altgard puts the game's mobs on them, 2.4 m off at the median.

**Gathering** puts a dot wherever a material is: Orichalcum ore, gems (diamond, ruby,
sapphire), herbs, logs, berries, vegetables, shellfish and Odyle, each kind in its own
colour; and the collectables, hidden cubes and Empyrean traces (hidden at first: there are
a lot of them). The list has this map's materials by kind with how many spots each has;
**✓** shows or hides one on the map, or a whole kind from its row, and the map remembers
your choice. Pick one, or click a dot, to light up its spots and see the areas with the
most. The spots are Questlog's (about 13,600); the ones in the game's own files sit on
them, 0.4 m off at the median.

Only the map showing is kept in memory, drawn as Questlog draws its own: in levels of
detail, a piece at a time.

## Names

Players are named when they come into view, after a zone change, or when they start a
fight; until then they show as `#ID`. Mobs and NPCs are named from the game data; the few
it doesn't have (and mobs already there when the meter started) show as **Unknown**, with
their type in the tooltip: double-click the target on the meter to name one. You're
recognised on your first hit on a real mob.
