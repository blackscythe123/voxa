# voxa-nudge: a tiny on-device model for "come back" reminders

Voxa is a voice keyboard. People who stop dictating for a few days often never come back, and a
well timed reminder can help, but a badly timed one annoys people into turning notifications off.
This crate decides **when** to send a re-engagement reminder and **which wording** to use, on the
phone, in under a microsecond, with no server and no ML framework.

- **Model**: a 13-connection neural network (pruned from 195 possible), evolved with a
  NEAT-inspired genetic algorithm inside a simulator of 5,000 synthetic users.
- **Personalisation**: Thompson sampling with Beta posteriors over message styles and send-time
  slots, using the network's output as the prior, updated from taps and swipes on the device.
- **Safety**: hard rules in plain code that the model cannot override.
- **Size**: `model/champion.json` is 661 bytes. Inference takes 65 to 90 ns per decision on a laptop.
- **Language**: Rust, dependencies are `serde` and `serde_json` only.

Results are from a simulator, not from real users. Read [Limitations](#honest-limitations)
before quoting them.

## The problem

Every couple of hours the app's background job asks: should Voxa remind this person now, and if
so, with what words? The four wordings (plain sentence case, no emojis, no dashes, no guilt):

| Style | Title | Body |
|---|---|---|
| Plain | Ready when you are | Tap the mic on the Voxa keyboard and say what you want to write. |
| Progress | Your time saved this month | You've saved 2h 10m this month by speaking instead of typing. |
| Tip | A quick Voxa tip | Say "new line" while you talk to start a new line. (rotates) |
| Quiet | We'll stay quiet | This is our last reminder for now. Voxa is on your keyboard whenever you want it. |

The Quiet wording is a promise, and the code keeps it: after it, nothing is sent until the user
dictates again.

## Approach

### 1. Hard rules first (`src/rules.rs`)

Checked before the network runs; when a rule blocks, the model is not even asked.

- At most one reminder every 3 days.
- Never between 21:00 and 09:00 local time.
- Never within 48 hours of a dictation.
- After 2 ignored or dismissed reminders in a row, stop until the user dictates again.
- After the Quiet reminder, stop until the user dictates again.
- A global off switch (the "Reminders" setting).

Each rule has its own unit test, and the simulator runs every policy through the same function.

### 2. Features (`src/features.rs`)

15 inputs, all in [-1, 1], built by the same code on the phone and in the simulator: hour of day
and day of week (sin/cos), days since the last dictation, the user's usual dictation hour (circular
mean, sin/cos), closeness of now to that usual hour (cos of the difference), fraction of the last
28 days with a dictation, reminders in the last 14 days, the last reminder's outcome
(opened / ignored / dismissed, one-hot), hours since the last reminder, and a bias.

The "closeness" input is deliberate feature engineering: a network this small cannot multiply the
two clock encodings itself, and timing is the main thing it has to learn.

### 3. The network (`src/net.rs`)

A fixed maximum topology, 15 inputs to 6 tanh hidden units to 5 outputs plus direct input to
output connections, 195 connections in all, each with an on/off mask. Outputs: P(send now)
through a sigmoid, and a softmax over the 4 styles. Masks let evolution add and remove
connections and hidden nodes NEAT style while every genome stays aligned for crossover.

### 4. Neuroevolution (`src/evolve.rs`)

- Population 96, 100 generations, seed 1. Deterministic: the population is scored on several
  threads, but every score depends only on the genome, so runs repeat bit for bit.
- Tournament selection (size 3), 2 elites copied unchanged, uniform crossover of aligned genes.
- Mutations: Gaussian weight nudges, fresh weights, add connection, remove connection, and NEAT's
  add node (split a direct input to output connection through an unused hidden node, incoming
  weight 1, outgoing weight the old one).
- Starts sparse (15% of connections on), like NEAT's minimal start.
- Fitness: the simulator score on 5,000 training users, with the hard rules on (the deployed
  condition).
- The champion is chosen on 1,500 separate validation users, then **pruned**: every connection is
  tried for removal, smallest weight first, and the removal is kept when the validation score does
  not go down. **24 connections before pruning, 13 after, same validation score (11.472).**

Unit tests cover the invariants: mutation keeps shape and finite weights, add node really splits a
connection, crossover takes every gene from one parent, pruning never lowers fitness and never adds
a connection, removing dangling connections never changes the output, same seed gives the same
champion, and elitism makes the best score non-decreasing.

### 5. The simulator (`src/sim.rs`)

5,000 synthetic users per pool (training, validation and test pools are disjoint). Each lives 14
warm-up days, to build a history, then 28 days in which reminders may go out. The policy is asked
every 2 hours from 07:00 to 23:00.

Users differ by:

- **Rhythm**: morning (22%), midday (16%), evening (22%), weekday-only workers (15%), weekend users
  (10%), night owls whose usual hour is about 22:00 (15%).
- **Habit**: daily chance of dictating starts between 0.25 and 0.85; it grows a little on days
  they dictate and decays on days they don't (35% are "lapsing" users whose habit decays fast).
- **Attitude to notifications**: 15% dislike them (low receptivity, high annoyance).
- **Wording taste**: a random affinity per style, with assumed links: heavier users like
  progress, lapsing users like tips, people who dislike notifications respond to the Quiet one.

A reminder is opened with a probability that falls off with the distance from the user's usual
hour, and depends on their receptivity, wording taste, the day, how strong their habit still is,
and their current annoyance. An opened reminder means a dictation that day and a boost to the
habit. Every reminder adds annoyance (more at bad times, at night, or right after they used the
app); annoyance fades by 7% a day, wears the habit down while it is high, and past a threshold
the user turns notifications off.

Randomness that belongs to the user (whether they dictate on a day, when, whether they open a
given reminder) comes from a hash of (user, day, check). Two policies therefore see the same people
living the same days, and differences come from the policies, not luck ("common random numbers").

**Score per user** = active days in the 28 days, minus 0.3 per dismissal, 1 per night send, 0.5
per send within 48 hours of a dictation, 3 per user who turned notifications off, and 0.05 per
reminder.

## Results

`cargo run -p voxa-nudge --release --bin train` (about 75 seconds on a laptop; full output in
`model/train_output.txt`, learning curve in `model/learning_curve.csv`).

Learning curve (best and mean training score per generation): 10.76 / 10.21 at generation 0,
11.54 / 10.84 at generation 7, 11.67 / 10.92 at generation 22, then flat to generation 100. The
search converged early; this problem is small.

Held-out test, 5,000 new simulated users, 28 reminder days each:

| Policy | Score per user | Active days per user | Reminders per user | Open rate | Dismissals per user | Opted out | Night sends per user | Lapsed at end |
|---|---|---|---|---|---|---|---|---|
| Never remind | 10.07 | 10.07 | 0.00 | 0% | 0.00 | 0.0% | 0.00 | 26.2% |
| Fixed 19:00 every 3 days, no rules | 3.57 | 9.11 | 10.00 | 5% | 2.25 | 65.3% | 0.00 | 29.4% |
| Fixed 19:00 every 3 days + rules | 9.52 | 10.43 | 4.28 | 9% | 1.09 | 12.2% | 0.00 | 25.7% |
| Random (1 in 27 checks), no rules | -0.12 | 8.69 | 9.32 | 2% | 1.55 | 84.7% | 3.09 | 32.1% |
| Random (1 in 10 checks) + rules | 9.76 | 10.36 | 2.82 | 8% | 0.69 | 8.3% | 0.00 | 25.1% |
| First allowed moment + rules | 9.97 | 10.91 | 4.79 | 13% | 1.15 | 11.8% | 0.00 | 23.9% |
| Heuristic: usual hour + rules | 10.94 | 11.51 | 3.83 | 22% | 0.87 | 3.9% | 0.00 | 20.3% |
| Heuristic: usual hour, tip wording + rules | 11.38 | 11.94 | 3.89 | 27% | 0.84 | 3.8% | 0.00 | 17.6% |
| **Evolved network + rules** | **11.40** | **11.95** | 3.94 | **27%** | 0.85 | **3.3%** | 0.00 | 17.7% |
| Evolved network + bandit + rules | 11.23 | 11.73 | 3.67 | 25% | 0.78 | 2.9% | 0.00 | 19.1% |

Paired differences (evolved network + rules minus baseline, per user, 95% interval):

| Baseline | Difference |
|---|---|
| Never remind | +1.32 [+1.24, +1.41] |
| Fixed 19:00 every 3 days + rules | +1.87 [+1.78, +1.96] |
| Heuristic: usual hour (plain wording) + rules | +0.46 [+0.40, +0.52] |
| Heuristic: usual hour, tip wording + rules | +0.02 [-0.03, +0.06] (no real difference) |
| Evolved network + bandit + rules | +0.17 [+0.12, +0.21] |

What the numbers say:

- **The rules alone do most of the harm reduction.** The same 19:00 reminder goes from a score of
  3.57 to 9.52 just by passing through the rules (opt-outs 65% to 12%).
- **Timing is what makes reminders worth sending.** Fixed-time and random reminders, even with the
  rules, score below sending nothing. Reminders near each user's usual hour beat sending nothing.
- **The evolved model beats the fixed schedule by 1.87 active days per user per month and never
  reminding by 1.32**, with the lowest opt-out rate of the policies that send anything (3.3%).
- **It found a rule a person could write, and tuned it.** Read back, the 13 connections say: send
  when it is within about 2 hours of the user's usual dictation hour (the rules handle the rest);
  use the tip wording, except that after a dismissal its lead almost vanishes and the progress
  wording wins for many afternoon and evening users. My hand-written "usual hour"
  rule with the plain wording scored 0.46 lower; once I copied the model's wording choice into
  the heuristic, the two tie (+0.02, not significant). So in this simulator the evolution's value
  was discovering the window width and the wording, not a strategy beyond human reach.
- **Not everyone gains.** For people who dislike notifications, never reminding still scores best
  (10.27 versus 8.96); for night owls (usual hour inside the quiet window) it is a wash.

By segment (score per user, held-out):

| Segment | Users | Never | Fixed 19:00 + rules | Usual hour + rules | Evolved + rules | Evolved + bandit |
|---|---|---|---|---|---|---|
| Morning people | 1050 | 11.69 | 10.24 | 12.98 | 13.44 | 13.21 |
| Midday | 791 | 11.23 | 10.22 | 12.29 | 12.99 | 12.77 |
| Evening people | 1112 | 11.43 | 12.72 | 12.61 | 13.17 | 12.93 |
| Weekday only | 753 | 7.54 | 5.90 | 8.32 | 8.86 | 8.72 |
| Weekend users | 496 | 3.36 | 2.53 | 3.67 | 3.87 | 3.77 |
| Night owls | 798 | 11.49 | 11.21 | 11.59 | 11.73 | 11.73 |
| Lapsing | 1799 | 5.53 | 5.31 | 6.77 | 7.98 | 7.43 |
| Dislike notifications | 775 | 10.27 | 6.06 | 8.70 | 8.96 | 9.08 |

### On-device personalisation: an insurance policy, measured

The bandit (`src/bandit.rs`) keeps successes and failures per style and per send-time slot
(09-12, 12-15, 15-18, 18-21). Styles: Thompson sampling, prior mean from the network's softmax
(worth 4 pseudo-observations). Slots: once a slot has 3 outcomes, a Thompson draw of its open rate
against the user's pooled rate shifts the network's send log-odds (half weight; centred with the
digamma function so it is zero on average when the slot is typical). The Quiet wording is never
explored at random, because it silences reminders.

To test it honestly I also built a **shifted world** (`sim::population_shifted`) where the
simulator's assumptions are wrong in ways the network cannot see: people read reminders 5 hours
away from when they dictate, and tips work much worse while the plain wording works much better.

| World | Reminder days | Network | Network + bandit | Network open rate | Network + bandit open rate |
|---|---|---|---|---|---|
| as trained | 28 | 11.37 | 11.22 | 27.0% | 26.2% |
| as trained | 182 | 10.43 | 10.13 | 26.8% | 26.8% |
| shifted | 28 | 9.00 | 9.27 | 1.8% | 4.6% |
| shifted | 182 | 7.34 | 8.15 | 1.8% | 6.1% |

(score per user per 28 days, 2,000 held-out users)

When the simulator is right, exploration costs about 0.15 to 0.3 points; when it is wrong, the
bandit recovers 0.27 points in the first month and 0.81 over six months, and its open rate
triples. Real users will differ from my simulator, so the app ships with the bandit on. I tried
variants before settling: using the slot posterior mean instead of a sample (no exploration) let
slots that failed early go silent for good and cut reminders by half; applying slot evidence from
the first outcome cost more than it earned.

## Honest limitations

- **It is a model of users, not data.** Every behaviour above (the habit dynamics, how timing
  affects opens, how annoyance builds and leads to opt-outs, the style tastes and how they correlate
  with user type, the size of the penalties) is an assumption I chose to be plausible. The
  absolute numbers mean nothing outside the simulator; the comparisons are only as good as the
  assumptions. The model's real test is on-device outcomes, which is why the bandit exists.
- **The learned policy is simple.** It matches a hand-written heuristic once that heuristic uses
  the same wording. The project's honest claim is "evolution found and tuned an interpretable
  13-parameter rule", not "the network discovered something a person could not".
- **Users with no history are never reminded** (the model keys on the usual hour). Onboarding
  reminders for people who installed but never dictated are a different problem.
- **One check grid.** The model was trained on decisions every 2 hours; a much different schedule
  on the phone changes how often it fires. The wiring doc asks for a 2-hour periodic job.
- **Short horizon for the bandit.** With at most one reminder every 3 days, the bandit learns
  slowly; most users will never give it more than a handful of outcomes per arm.
- **No offline evaluation on real logs yet.** If Voxa ever has consented reminder logs, the next
  step is off-policy evaluation (inverse propensity scoring) of the shipped policy against
  alternatives, and refitting the simulator's parameters to those logs.

## Using it

```rust
use voxa_nudge::{Engine, Usage, rng::Rng};

let mut engine = Engine::open(data_dir);            // state in data_dir/nudge.json
let usage = Usage { dictations_ms, saved_secs_month };
match engine.decide(now_ms, utc_offset_minutes, &usage, &mut Rng::new(now_ms)) {
    Ok(nudge) => post_notification(&nudge.title, &nudge.body),
    Err(_skip) => {} // a rule blocked it, or the model chose to wait
}
// later, from the notification:
engine.record_outcome(voxa_nudge::Outcome::Opened);
```

Android goes through `ReminderEngine` in `core/voxa-ffi/src/nudge.rs`, which reads History
itself; the wiring (channel, periodic job, tap and swipe, the Settings switch) and the desktop
plan are in `docs/reminders.md`.

Retrain: `cargo run -p voxa-nudge --release --bin train -- --generations 100 --population 96
--seed 1`. Tests: `cargo test -p voxa-nudge`.
