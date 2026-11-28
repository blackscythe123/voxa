//! Neuroevolution, NEAT inspired and kept simple: a population of masked networks, tournament
//! selection with elitism, uniform crossover of aligned genes, Gaussian weight mutation, add and
//! remove connection mutations, NEAT's add node mutation (split a direct connection through an
//! unused hidden node), and a pruning pass that removes every connection the fitness does not
//! need. Deterministic for a given seed, also when the population is scored on several threads.

use crate::features::N_IN;
use crate::net::{Conn, N_GENES, N_HID, N_OUT, Net, conn, gene_hid_out, gene_in_hid};
use crate::rng::Rng;

#[derive(Clone, Debug)]
pub struct Config {
    pub population: usize,
    pub generations: usize,
    /// Best genomes copied unchanged into the next generation.
    pub elites: usize,
    pub tournament: usize,
    pub crossover_rate: f64,
    /// Per enabled connection: chance of a Gaussian nudge, its size, and chance of a fresh weight.
    pub weight_mut_rate: f64,
    pub weight_sigma: f64,
    pub weight_reset_rate: f64,
    /// Per child: chance of each structural mutation.
    pub add_conn_rate: f64,
    pub del_conn_rate: f64,
    pub add_node_rate: f64,
    /// Share of connections enabled in the first generation (NEAT starts small).
    pub init_density: f64,
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            population: 64,
            generations: 60,
            elites: 2,
            tournament: 3,
            crossover_rate: 0.5,
            weight_mut_rate: 0.2,
            weight_sigma: 0.3,
            weight_reset_rate: 0.02,
            add_conn_rate: 0.4,
            del_conn_rate: 0.25,
            add_node_rate: 0.1,
            init_density: 0.15,
            seed: 1,
        }
    }
}

pub fn random_net(rng: &mut Rng, density: f64) -> Net {
    let mut n = Net::default();
    for g in 0..N_GENES {
        n.w[g] = (rng.normal() * 0.5) as f32;
        n.on[g] = rng.chance(density);
    }
    n
}

fn hidden_unused(n: &Net, h: usize) -> bool {
    (0..N_IN).all(|i| !n.on[gene_in_hid(i, h)]) && (0..N_OUT).all(|o| !n.on[gene_hid_out(h, o)])
}

/// NEAT's add node: an enabled input to output connection is split through an unused hidden node.
/// The incoming weight is 1 and the outgoing weight is the old one, so the new path starts close to
/// the old behaviour. Returns false when there is no direct connection or no free hidden node.
pub fn add_node(n: &mut Net, rng: &mut Rng) -> bool {
    let direct: Vec<usize> = (0..N_GENES).filter(|&g| n.on[g] && matches!(conn(g), Conn::InOut { .. })).collect();
    let free: Vec<usize> = (0..N_HID).filter(|&h| hidden_unused(n, h)).collect();
    if direct.is_empty() || free.is_empty() {
        return false;
    }
    let g = direct[rng.below(direct.len())];
    let h = free[rng.below(free.len())];
    let Conn::InOut { i, o } = conn(g) else { return false };
    n.on[g] = false;
    let (a, b) = (gene_in_hid(i, h), gene_hid_out(h, o));
    n.on[a] = true;
    n.w[a] = 1.0;
    n.on[b] = true;
    n.w[b] = n.w[g];
    true
}

pub fn mutate(n: &mut Net, cfg: &Config, rng: &mut Rng) {
    for g in 0..N_GENES {
        if !n.on[g] {
            continue;
        }
        if rng.chance(cfg.weight_reset_rate) {
            n.w[g] = rng.normal() as f32;
        } else if rng.chance(cfg.weight_mut_rate) {
            n.w[g] = (n.w[g] as f64 + rng.normal() * cfg.weight_sigma).clamp(-8.0, 8.0) as f32;
        }
    }
    if rng.chance(cfg.add_conn_rate) {
        let off: Vec<usize> = (0..N_GENES).filter(|&g| !n.on[g]).collect();
        if !off.is_empty() {
            let g = off[rng.below(off.len())];
            n.on[g] = true;
            n.w[g] = (rng.normal() * 0.5) as f32;
        }
    }
    if rng.chance(cfg.del_conn_rate) {
        let on: Vec<usize> = (0..N_GENES).filter(|&g| n.on[g]).collect();
        if !on.is_empty() {
            n.on[on[rng.below(on.len())]] = false;
        }
    }
    if rng.chance(cfg.add_node_rate) {
        add_node(n, rng);
    }
}

/// Uniform crossover. Genes are aligned by position (the fixed topology), so each connection's
/// weight and switch come together from one parent.
pub fn crossover(a: &Net, b: &Net, rng: &mut Rng) -> Net {
    let mut c = a.clone();
    for g in 0..N_GENES {
        if rng.chance(0.5) {
            c.w[g] = b.w[g];
            c.on[g] = b.on[g];
        }
    }
    c
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenStats {
    pub generation: usize,
    pub best: f64,
    pub mean: f64,
    pub best_params: usize,
    pub best_hidden: usize,
}

pub struct Evolved {
    /// The last generation, best first, with its scores.
    pub population: Vec<(Net, f64)>,
    /// The best genome of every generation.
    pub hall_of_fame: Vec<(Net, f64)>,
    pub history: Vec<GenStats>,
}

/// Scores every genome. Threads only split the work; each score depends on the genome and the
/// generation alone, so the result is the same on any machine.
fn score_all(pop: &[Net], generation: usize, fitness: &(dyn Fn(&Net, usize) -> f64 + Sync)) -> Vec<f64> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 16);
    let chunk = pop.len().div_ceil(threads).max(1);
    let mut out = vec![0.0; pop.len()];
    std::thread::scope(|s| {
        for (nets, scores) in pop.chunks(chunk).zip(out.chunks_mut(chunk)) {
            s.spawn(move || {
                for (n, f) in nets.iter().zip(scores.iter_mut()) {
                    *f = fitness(n, generation);
                }
            });
        }
    });
    out
}

fn tournament<'a>(pop: &'a [Net], scores: &[f64], k: usize, rng: &mut Rng) -> &'a Net {
    let mut best = rng.below(pop.len());
    for _ in 1..k {
        let c = rng.below(pop.len());
        if scores[c] > scores[best] {
            best = c;
        }
    }
    &pop[best]
}

fn ranked(pop: Vec<Net>, scores: Vec<f64>) -> Vec<(Net, f64)> {
    let mut v: Vec<(Net, f64)> = pop.into_iter().zip(scores).collect();
    // Stable sort: ties keep population order, so runs repeat exactly.
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// Runs the evolution. `fitness(net, generation)` must be deterministic; it may change with the
/// generation (a fresh batch of simulated users each time). `log` sees each generation's stats.
pub fn evolve(cfg: &Config, fitness: &(dyn Fn(&Net, usize) -> f64 + Sync), mut log: impl FnMut(&GenStats)) -> Evolved {
    let mut rng = Rng::new(cfg.seed);
    let mut pop: Vec<Net> = (0..cfg.population).map(|_| random_net(&mut rng, cfg.init_density)).collect();
    let mut history = Vec::new();
    let mut hall_of_fame = Vec::new();
    let mut last = Vec::new();
    for generation in 0..cfg.generations {
        let scores = score_all(&pop, generation, fitness);
        let r = ranked(pop, scores);
        let stats = GenStats {
            generation,
            best: r[0].1,
            mean: r.iter().map(|x| x.1).sum::<f64>() / r.len() as f64,
            best_params: r[0].0.params(),
            best_hidden: r[0].0.active_hidden(),
        };
        log(&stats);
        history.push(stats);
        hall_of_fame.push(r[0].clone());
        let (nets, scores): (Vec<Net>, Vec<f64>) = r.iter().cloned().unzip();
        if generation + 1 == cfg.generations {
            last = r;
            break;
        }
        let mut next: Vec<Net> = nets.iter().take(cfg.elites).cloned().collect();
        while next.len() < cfg.population {
            let mut child = if rng.chance(cfg.crossover_rate) {
                let a = tournament(&nets, &scores, cfg.tournament, &mut rng);
                let b = tournament(&nets, &scores, cfg.tournament, &mut rng);
                crossover(a, b, &mut rng)
            } else {
                tournament(&nets, &scores, cfg.tournament, &mut rng).clone()
            };
            mutate(&mut child, cfg, &mut rng);
            next.push(child);
        }
        pop = next;
    }
    Evolved { population: last, hall_of_fame, history }
}

/// Switches off connections that cannot change the output: into a hidden node with no way out, or
/// out of a hidden node with no way in (tanh(0) = 0).
pub fn drop_dangling(n: &mut Net) {
    for h in 0..N_HID {
        let has_in = (0..N_IN).any(|i| n.on[gene_in_hid(i, h)]);
        let has_out = (0..N_OUT).any(|o| n.on[gene_hid_out(h, o)]);
        if !has_in {
            (0..N_OUT).for_each(|o| n.on[gene_hid_out(h, o)] = false);
        }
        if !has_out {
            (0..N_IN).for_each(|i| n.on[gene_in_hid(i, h)] = false);
        }
    }
}

pub struct Pruned {
    pub net: Net,
    pub params_before: usize,
    pub params_after: usize,
    pub fitness_before: f64,
    pub fitness_after: f64,
}

/// Greedy pruning: try removing each connection, smallest weight first, and keep the removal when
/// the fitness does not go down. Never lowers the fitness and never adds a connection.
pub fn prune(net: &Net, fitness: &dyn Fn(&Net) -> f64) -> Pruned {
    let mut n = net.clone();
    let params_before = n.params();
    let fitness_before = fitness(&n);
    let mut best = fitness_before;
    drop_dangling(&mut n);
    let f = fitness(&n);
    debug_assert!(f == best, "dangling connections changed the output");
    best = best.max(f);
    let mut order: Vec<usize> = (0..N_GENES).filter(|&g| n.on[g]).collect();
    order.sort_by(|&a, &b| n.w[a].abs().total_cmp(&n.w[b].abs()).then(a.cmp(&b)));
    for g in order {
        if !n.on[g] {
            continue;
        }
        let mut t = n.clone();
        t.on[g] = false;
        drop_dangling(&mut t);
        let f = fitness(&t);
        if f >= best {
            best = f;
            n = t;
        }
    }
    Pruned { params_after: n.params(), net: n, params_before, fitness_before, fitness_after: best }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::gene_in_out;

    fn cfg() -> Config {
        Config { population: 24, generations: 12, seed: 9, ..Config::default() }
    }

    /// A toy task: P(send) should follow input 0 (sigmoid of 3 x0) over a grid of inputs.
    fn toy_fitness(n: &Net, _gen: usize) -> f64 {
        let mut err = 0.0;
        for k in 0..21 {
            let v = -1.0 + 0.1 * k as f32;
            let mut x = [0.0f32; N_IN];
            x[0] = v;
            x[N_IN - 1] = 1.0;
            let target = 1.0 / (1.0 + (-3.0 * v).exp());
            err += ((n.forward(&x).p_send - target) as f64).powi(2);
        }
        -err
    }

    #[test]
    fn mutation_keeps_the_shape_and_finite_weights() {
        let c = Config { add_conn_rate: 1.0, del_conn_rate: 1.0, add_node_rate: 1.0, weight_mut_rate: 1.0, ..cfg() };
        let mut rng = Rng::new(1);
        let mut n = random_net(&mut rng, 0.3);
        for _ in 0..500 {
            mutate(&mut n, &c, &mut rng);
            assert_eq!(n.w.len(), N_GENES);
            assert_eq!(n.on.len(), N_GENES);
            assert!(n.w.iter().all(|w| w.is_finite()));
        }
    }

    #[test]
    fn add_node_splits_a_direct_connection() {
        let mut n = Net::default();
        let g = gene_in_out(3, 2);
        n.on[g] = true;
        n.w[g] = 0.7;
        let mut rng = Rng::new(2);
        assert!(add_node(&mut n, &mut rng));
        assert!(!n.on[g], "the split connection is switched off");
        assert_eq!(n.params(), 2);
        assert_eq!(n.active_hidden(), 1);
        let h = (0..N_HID).find(|&h| n.on[gene_in_hid(3, h)]).unwrap();
        assert_eq!(n.w[gene_in_hid(3, h)], 1.0);
        assert!(n.on[gene_hid_out(h, 2)]);
        assert_eq!(n.w[gene_hid_out(h, 2)], 0.7);
        // No direct connections left: nothing to split.
        assert!(!add_node(&mut n, &mut rng));
    }

    #[test]
    fn add_node_needs_a_free_hidden_node() {
        let mut n = Net::default();
        for h in 0..N_HID {
            n.on[gene_in_hid(0, h)] = true;
        }
        n.on[gene_in_out(1, 1)] = true;
        assert!(!add_node(&mut n, &mut Rng::new(3)));
    }

    #[test]
    fn crossover_takes_each_gene_from_a_parent() {
        let mut rng = Rng::new(4);
        let a = random_net(&mut rng, 0.5);
        let b = random_net(&mut rng, 0.5);
        let c = crossover(&a, &b, &mut rng);
        let (mut from_a, mut from_b) = (0, 0);
        for g in 0..N_GENES {
            let is_a = c.w[g] == a.w[g] && c.on[g] == a.on[g];
            let is_b = c.w[g] == b.w[g] && c.on[g] == b.on[g];
            assert!(is_a || is_b, "gene {g} came from neither parent");
            from_a += is_a as usize;
            from_b += is_b as usize;
        }
        assert!(from_a > 40 && from_b > 40);
        assert_eq!(crossover(&a, &a, &mut rng), a);
    }

    #[test]
    fn pruning_never_hurts_and_keeps_what_matters() {
        let mut rng = Rng::new(5);
        let mut n = random_net(&mut rng, 0.6);
        let key = gene_in_out(0, 0);
        n.on[key] = true;
        n.w[key] = 2.0;
        // Fitness only cares that the key connection is there and positive.
        let f = |n: &Net| if n.on[key] && n.w[key] > 0.0 { 1.0 } else { 0.0 };
        let p = prune(&n, &f);
        assert_eq!(p.params_after, 1);
        assert!(p.net.on[key]);
        assert!(p.fitness_after >= p.fitness_before);
        assert!(p.params_after <= p.params_before);
        // On a real objective too.
        let n = evolve(&cfg(), &toy_fitness, |_| {}).population[0].0.clone();
        let p = prune(&n, &|n| toy_fitness(n, 0));
        assert!(p.fitness_after >= p.fitness_before);
        assert!(p.params_after <= p.params_before);
        for g in 0..N_GENES {
            assert!(!p.net.on[g] || n.on[g], "pruning added a connection");
        }
    }

    #[test]
    fn dangling_connections_change_nothing() {
        let mut rng = Rng::new(6);
        let n = random_net(&mut rng, 0.3);
        let mut d = n.clone();
        drop_dangling(&mut d);
        let x = [0.4; N_IN];
        assert_eq!(d.forward(&x), n.forward(&x));
    }

    #[test]
    fn evolution_learns_and_repeats_exactly() {
        let a = evolve(&cfg(), &toy_fitness, |_| {});
        let b = evolve(&cfg(), &toy_fitness, |_| {});
        assert_eq!(a.population[0].0, b.population[0].0, "same seed, same champion");
        assert_eq!(a.history, b.history);
        let first = a.history[0].best;
        let last = a.history.last().unwrap().best;
        assert!(last > first, "fitness went from {first} to {last}");
        // Elitism: the best score on a fixed objective never goes down.
        for w in a.history.windows(2) {
            assert!(w[1].best >= w[0].best);
        }
        let c = evolve(&Config { seed: 10, ..cfg() }, &toy_fitness, |_| {});
        assert_ne!(a.population[0].0, c.population[0].0);
    }
}
