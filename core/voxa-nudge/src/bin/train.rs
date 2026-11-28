//! Evolves the reminder model in the simulator, prunes it, saves it and compares it with baselines
//! on held-out simulated users.
//!
//!     cargo run -p voxa-nudge --release --bin train [-- --generations 60 --population 64 --seed 1]

use std::fmt::Write as _;
use std::time::Instant;

use voxa_nudge::evolve::{self, Config};
use voxa_nudge::features::{N_IN, encode};
use voxa_nudge::net::{ModelNote, N_GENES, Net};
use voxa_nudge::Style;
use voxa_nudge::sim::{self, FirstChance, FixedTime, Kind, NetPolicy, Never, Policy, Random, SimConfig, User, UsualHour};

const TRAIN_USERS: usize = 5000;
const VALIDATION_USERS: usize = 1500;
const TEST_USERS: usize = 5000;

fn arg(name: &str) -> Option<u64> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

fn row(name: &str, t: &sim::Tally) -> String {
    format!(
        "| {name} | {:.2} | {:.2} | {:.2} | {:.0}% | {:.2} | {:.1}% | {:.2} | {:.1}% |",
        t.score(),
        t.per_user(t.active_days),
        t.per_user(t.sends),
        100.0 * t.open_rate(),
        t.per_user(t.dismissals),
        100.0 * t.per_user(t.opt_outs),
        t.per_user(t.night_sends),
        100.0 * t.per_user(t.lapsed),
    )
}

fn main() {
    let cfg = Config {
        generations: arg("--generations").unwrap_or(60) as usize,
        population: arg("--population").unwrap_or(64) as usize,
        seed: arg("--seed").unwrap_or(1),
        ..Config::default()
    };
    let train = sim::population(TRAIN_USERS, 101);
    let validation = sim::population(VALIDATION_USERS, 202);
    let test = sim::population(TEST_USERS, 303);

    println!("voxa-nudge neuroevolution: population {}, {} generations, seed {}", cfg.population, cfg.generations, cfg.seed);
    println!("network: {N_IN} inputs (incl. bias), 6 hidden max, 5 outputs, {N_GENES} possible connections");
    println!("simulator: {TRAIN_USERS} training users (all of them every generation), {VALIDATION_USERS} validation, {TEST_USERS} held-out test");
    println!();
    println!("gen   best    mean    params hidden");
    let started = Instant::now();
    // Every genome is scored on the same training users (common random numbers), so the best
    // score can only go up (elitism); validation users pick the champion, test users report.
    let fitness = |n: &Net, _generation: usize| sim::net_score(n, &train);
    let mut curve = String::from("generation,best,mean,best_params,best_hidden\n");
    let evolved = evolve::evolve(&cfg, &fitness, |s| {
        println!("{:>3}  {:>6.3}  {:>6.3}  {:>6} {:>6}", s.generation, s.best, s.mean, s.best_params, s.best_hidden);
        let _ = writeln!(curve, "{},{:.4},{:.4},{},{}", s.generation, s.best, s.mean, s.best_params, s.best_hidden);
    });
    println!("evolution took {:.1} s", started.elapsed().as_secs_f64());

    // Pick the champion on validation users: the final top 5 and the last 10 generation bests.
    let mut candidates: Vec<Net> = evolved.population.iter().take(5).map(|x| x.0.clone()).collect();
    candidates.extend(evolved.hall_of_fame.iter().rev().take(10).map(|x| x.0.clone()));
    let (champion, val) = candidates
        .into_iter()
        .map(|n| {
            let f = sim::net_score(&n, &validation);
            (n, f)
        })
        .fold(None::<(Net, f64)>, |best, c| match best {
            Some(b) if b.1 >= c.1 => Some(b),
            _ => Some(c),
        })
        .unwrap();
    println!("champion: validation score {val:.3}, {} connections, {} hidden nodes", champion.params(), champion.active_hidden());

    let pruned = evolve::prune(&champion, &|n| sim::net_score(n, &validation));
    let model = pruned.net.rounded();
    println!(
        "pruning: {} -> {} connections, validation score {:.3} -> {:.3} (after rounding: {:.3})",
        pruned.params_before,
        pruned.params_after,
        pruned.fitness_before,
        pruned.fitness_after,
        sim::net_score(&model, &validation)
    );

    let note = ModelNote {
        seed: cfg.seed,
        generations: cfg.generations,
        population: cfg.population,
        params_before_pruning: pruned.params_before,
        validation_fitness: (sim::net_score(&model, &validation) * 1000.0).round() / 1000.0,
    };
    let json = model.to_json(note);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("model");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("champion.json"), &json).expect("write champion.json");
    std::fs::write(dir.join("learning_curve.csv"), &curve).expect("write learning_curve.csv");
    println!("saved model/champion.json ({} bytes) and model/learning_curve.csv", json.len());

    // Inference speed.
    let x = encode(&voxa_nudge::features::context(1_765_000_000_000, 0, &[1_764_000_000_000], &Default::default()));
    let reps = 1_000_000;
    let t0 = Instant::now();
    let mut acc = 0.0f32;
    for k in 0..reps {
        let mut x = x;
        x[0] = (k % 100) as f32 / 100.0;
        acc += model.forward(&x).p_send;
    }
    let ns = t0.elapsed().as_nanos() as f64 / reps as f64;
    println!("inference: {ns:.0} ns per decision (checksum {acc:.1})");

    // Held-out comparison.
    let rules = SimConfig::RULES;
    let no_rules = SimConfig::NO_RULES;
    type Make<'a> = Box<dyn Fn() -> Box<dyn Policy + 'a> + 'a>;
    let model_ref = &model;
    let policies: Vec<(&str, Make, SimConfig)> = vec![
        ("Never remind", Box::new(|| Box::new(Never)), rules),
        ("Fixed 19:00 every 3 days, no rules", Box::new(|| Box::new(FixedTime { hour: 19.0 })), no_rules),
        ("Fixed 19:00 every 3 days + rules", Box::new(|| Box::new(FixedTime { hour: 19.0 })), rules),
        ("Random (1 in 27 checks), no rules", Box::new(|| Box::new(Random { p: 1.0 / 27.0 })), no_rules),
        ("Random (1 in 10 checks) + rules", Box::new(|| Box::new(Random { p: 0.1 })), rules),
        ("Heuristic: usual hour + rules", Box::new(|| Box::new(UsualHour { style: Style::Plain })), rules),
        ("First allowed moment + rules", Box::new(|| Box::new(FirstChance)), rules),
        ("Evolved network + rules", Box::new(move || Box::new(NetPolicy { net: model_ref, personalise: false })), rules),
        (
            "Evolved network + bandit + rules",
            Box::new(move || Box::new(NetPolicy { net: model_ref, personalise: true })),
            rules,
        ),
        ("Heuristic: usual hour, tip wording + rules", Box::new(|| Box::new(UsualHour { style: Style::Tip })), rules),
    ];
    println!();
    println!("held-out test: {TEST_USERS} simulated users, 28 reminder days each");
    println!();
    println!(
        "| Policy | Score per user | Active days per user | Reminders per user | Open rate | Dismissals per user | Opted out | Night sends per user | Lapsed at end |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for (name, make, c) in &policies {
        let t = sim::evaluate(&test, &**make, *c);
        println!("{}", row(name, &t));
    }

    // Paired differences: the same users live the same days under both policies, so per-user
    // differences give a tight confidence interval.
    let per_user = |i: usize| -> Vec<f64> {
        test.iter()
            .map(|u| {
                let mut p = (policies[i].1)();
                sim::simulate_user(u, p.as_mut(), policies[i].2).score()
            })
            .collect()
    };
    let evolved = per_user(7);
    println!();
    println!("evolved network + rules minus baseline, score per user, held-out (mean and 95% interval):");
    for i in [0usize, 2, 5, 9, 8] {
        let base = per_user(i);
        let d: Vec<f64> = evolved.iter().zip(&base).map(|(a, b)| a - b).collect();
        let n = d.len() as f64;
        let mean = d.iter().sum::<f64>() / n;
        let sd = (d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let half = 1.96 * sd / n.sqrt();
        println!("  vs {:<44} {:+.2} [{:+.2}, {:+.2}]", policies[i].0, mean, mean - half, mean + half);
    }

    // Who gains: per segment.
    println!();
    println!("by segment (score per user, held-out):");
    println!();
    println!("| Segment | Users | Never | Fixed 19:00 + rules | Usual hour + rules | Evolved + rules | Evolved + bandit |");
    println!("|---|---|---|---|---|---|---|");
    let segments: Vec<(&str, Box<dyn Fn(&User) -> bool>)> = vec![
        ("Morning people", Box::new(|u: &User| u.kind == Kind::Morning)),
        ("Midday", Box::new(|u: &User| u.kind == Kind::Midday)),
        ("Evening people", Box::new(|u: &User| u.kind == Kind::Evening)),
        ("Weekday only", Box::new(|u: &User| u.kind == Kind::WeekdayWorker)),
        ("Weekend users", Box::new(|u: &User| u.kind == Kind::WeekendUser)),
        ("Night owls", Box::new(|u: &User| u.kind == Kind::NightOwl)),
        ("Lapsing", Box::new(|u: &User| u.lapsing)),
        ("Dislike notifications", Box::new(|u: &User| u.hates_notifications)),
    ];
    for (name, keep) in &segments {
        let group: Vec<User> = test.iter().filter(|u| keep(u)).cloned().collect();
        let s = |i: usize| sim::evaluate(&group, &*policies[i].1, policies[i].2).score();
        println!("| {name} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} |", group.len(), s(0), s(2), s(5), s(7), s(8));
    }

    // What the champion learned to say.
    let style_counts = {
        let mut counts = [0usize; 4];
        let mut users_sent = 0;
        for u in test.iter().take(1000) {
            let mut p = StyleCounter { net: model_ref, counts: [0; 4] };
            sim::simulate_user(u, &mut p, rules);
            if p.counts.iter().sum::<usize>() > 0 {
                users_sent += 1;
            }
            for k in 0..4 {
                counts[k] += p.counts[k];
            }
        }
        let total = counts.iter().sum::<usize>().max(1);
        Style::ALL
            .iter()
            .map(|s| format!("{} {:.0}%", s.name(), 100.0 * counts[s.index()] as f64 / total as f64))
            .collect::<Vec<_>>()
            .join(", ")
            + &format!(" (users reminded at least once: {users_sent} of 1000)")
    };
    println!();
    println!("evolved network's wording mix: {style_counts}");

    // Does on-device personalisation pay off? Same users over longer horizons, in the training
    // world and in a world where the simulator's assumptions are wrong (see population_shifted).
    println!();
    println!("on-device personalisation (held-out, 2000 users, score per user per 28 days):");
    println!();
    println!("| World | Reminder days | Network | Network + bandit | Network open rate | Network + bandit open rate |");
    println!("|---|---|---|---|---|---|");
    let worlds = [("as trained", test[..2000].to_vec()), ("shifted", sim::population_shifted(2000, 303))];
    for (world, users) in &worlds {
        for days in [28u64, 182] {
            let c = SimConfig { rules: true, eval_days: days };
            let a = sim::evaluate(users, &*policies[7].1, c);
            let b = sim::evaluate(users, &*policies[8].1, c);
            let k = 28.0 / days as f64;
            println!(
                "| {world} | {days} | {:.2} | {:.2} | {:.1}% | {:.1}% |",
                a.score() * k,
                b.score() * k,
                100.0 * a.open_rate(),
                100.0 * b.open_rate()
            );
        }
    }
}

/// The network without the bandit, counting the styles it sends.
struct StyleCounter<'a> {
    net: &'a Net,
    counts: [usize; 4],
}

impl Policy for StyleCounter<'_> {
    fn decide(
        &mut self,
        ctx: &voxa_nudge::features::Context,
        state: &voxa_nudge::NudgeState,
        rng: &mut voxa_nudge::rng::Rng,
    ) -> Option<voxa_nudge::Style> {
        let s = voxa_nudge::engine::choose(self.net, ctx, state, &[true; 4], false, rng).map(|x| x.0);
        if let Some(s) = s {
            self.counts[s.index()] += 1;
        }
        s
    }
}
