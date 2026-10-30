# Roadmap

Status: first written · Date: 2026-10-28 · Owner: andreagenovese

Companion to the [README](../README.md) (what quack-nav is), [`todo-map.md`](todo-map.md)
(the work in progress, with its numbers) and [`results.md`](results.md) (what was measured).
This is *order and direction* — it will change; the measurements won't.

The shape it follows is Pollen's own: `robotd` is a daemon with a typed, versioned contract
that anything above it — a gamepad, a console, a voice, an agent — uses without the daemon
knowing who they are. quack-nav wants to be the same kind of thing one floor up: the duck's
navigation and its actions, autonomous on the board, exposed as a service to whatever sits
above it.

## Where we are

| | |
|---|---|
| mapping | a map of the house from the head's ToF matrix, built while the duck walks, kept true across sessions, saved under the house's name — **shipped** |
| relocalization | the duck carried and switched on elsewhere finds itself on the saved map before it moves — **shipped** |
| guards | cliffs and drops seen before the step; a look at the floor ahead where the sensor cannot see; stops after every turn — **shipped** |
| exploration | the house mapped a charge at a time, room by room, from nothing, resumed where it stopped — **shipped** |
| journeys | `go_to` a named place through a planned route, people and pets as moving obstacles — **shipped** |
| the learned pilot | a walk trained on the simulator twin that beats the hand-written one on the twin — **shipped** |
| the tools | everything above as JSON-RPC tools on a local socket, with a catalogue, for a voice satellite, a control page and any agent — **shipped** |
| the twin | a MuJoCo copy of the duck in several houses, where every change is A/B'd against the frozen release before it ships — the method, not a feature |
| in flight | the gait ledger: the duck measures its own walk and keeps what it learned |
| missing | a typed contract with events and a client, actions beyond walking, limits set by the owner, a second sensor, a second body |

## Milestones

Each has a test that says when it is done ("Done when"); the status is in its title. The numbers are identifiers, not a sequence; [the order of
work](#the-order-of-work) is its own section.

### N1 — A duck that gets through · **done**

Map, relocalize, guard, explore, go. The first roadmap had one destination: a duck that maps a
house on its own sensors, finds itself in it, and walks where it is told without falling. It
arrived with v0.3.0.

**Done when:** the twin's houses, four tours each against the frozen release, every journey
arrived, no falls; the saved map relocalized from every wake spot.

### N2 — A walk that measures itself · **in progress**

The numbers of the walk — how fast a command really moves the body, how much it veers, how a
turn ends — are no longer constants per release: the duck learns them on its own legs, keeps
them beside the map, and starts every session from them. A daemon or policy update from
Pollen changes the walk; the duck adapts and keeps navigating. One set of numbers per state
of the body (a load in the beak changes the walk), the state read from the daemon. Left: the
one number the walk does not give as a number, and the test of a walk that really changed.

**Done when:** the twin on a newer daemon than the one the numbers were measured on, with the
ledger empty, maps and navigates as well as on its own; no file to edit, no release to cut.

### N3 — A body that stops where it says

A move answers when the body is still, with what it did, not when the command was sent. The
leg's last chunk and the tool's own end are held the way the turn's already is.

**Done when:** a `robot.move` of a stated length ends within a stated tolerance on the twin, and
the twin's A/B shows no regression.

### N4 — A service in robotd's style

A typed, versioned contract for everything quack-nav does, with a `hello` that says which
version is speaking; events pushed to whoever subscribed — a journey's progress, an arrival,
a refusal, a fall, the pose lost and found — instead of polling; a client library so the
voice satellite, the control page and a bridge stop writing JSON by hand; one owner at a time
when several clients want the duck; a queue of jobs the duck runs on its own, stopping at the
first that fails and reporting each. A protocol document with its own version.

**Done when:** the voice satellite and the control page run on the client library against the
contract's version check; an event reaches a subscriber before the next poll would have.

### N5 — Doing, not only walking

The duck carries out the skills its daemon already has — a pick, a kick, sit and stand — when
asked, as one job: walk near, square up at the trained distance, run the skill, wait for the
body to finish, say whether it worked. The skills are the daemon's own and are read from it
at run time, so whatever Pollen adds is offered as is.

**Done when:** the pick and the kick land from a navigation job as often as from the pad's button
by hand, measured on the twin and on the duck.

### N6 — Limits set by the owner

No-go zones drawn on the map; an exploration bounded to a rectangle; sessions sized by the
battery and the heat rather than by the clock; a charging spot as a named place the duck
returns to on its own before it must.

**Done when:** the duck never enters a no-go zone on the twin over the usual tours, and ends every
session at its charging spot with charge to spare.

### N7 — A second sensor

The camera, always optional: nothing in the guards depends on it, and nothing changes without
it. In this order — people and pets as tracked movers; recognizing the place from a picture,
so a wake elsewhere costs seconds; a record of the things seen and where, and a search job
that looks for one; then meaning — which room, what is on the floor. Small networks on the
board where they fit; the bigger ones on a server in the house, with the video never leaving
the house. The hard cases the duck meets feed the training of its own pilot, in a loop the
house's server runs.

**Done when:** the wake elsewhere relocalizes faster with the camera than without; a thing seen is
found again from the record; the twin's shields pass with the camera off.

### N8 — Another body

The navigation on a different biped. Everything quack-nav needs from a body is one small
interface — a velocity command and a stop, odometry, standing or fallen, a depth source that
becomes rays — documented as the contract to implement, with the duck as its first
implementation.

**Done when:** a second implementation maps and navigates a house on the twin.

## The order of work

N2 first: it is in flight and it is what every later change rides on. Then N3, which is
cheap and makes every job's answer honest. N4 and N5 next, together — the service is what the
actions are exposed through — with the small pieces of N6 slipped in where they cost little.
N7 after the known limits of the sensor are closed, in the order it states. N8 when a second
body is in sight.

## Decisions that shape work rather than follow it

- **Measure, don't code.** A number the duck can measure on itself is measured; a constant
  stays only as a prior with bounds.
- **The daemon owns the motors; the skills are its own.** quack-nav asks, never drives a
  joint, never ships a policy of its own for a skill.
- **Nothing above the duck is required.** A voice, a control page, an agent, a model: each is
  one client of the same contract; the duck navigates with none of them.
- **Falls are counted, never hidden.** Every change ships with its A/B against the frozen
  release and its shield test, misses included.
- **The house's video stays in the house.**
- **Priorities: get through, then the map, then relocalization, then the smooth walk.**

## Not doing, on purpose

- A visual SLAM in place of the ToF map: the sensor the duck has works, the camera adds to it.
- A model that drives: a model sets goals, the board reacts.
- Kicking an unknown thing out of the way on the duck's own initiative.
