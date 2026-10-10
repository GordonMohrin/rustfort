//! The simulation runs on its own thread, so "max" speed uses a full core without costing
//! frames.
//!
//! The UI thread owns the `Sim` while it handles input and builds the frame: it reads the
//! world to draw it and calls the sim API directly, exactly as before. Right before the GPU
//! flush and the vsync wait it hands the `Sim` to the worker together with a tick allowance
//! (`u64::MAX` at "max"), and it takes the `Sim` back at the start of the next frame. The
//! worker runs ticks in chunks of about half a millisecond and checks between chunks whether
//! the UI wants the sim back, so taking it back costs at most one chunk.
//!
//! There is never shared mutable state: the `Sim` is moved between the threads through
//! channels, so the UI code needs no locks and the simulation stays deterministic (the
//! same ticks run in the same order, only on another thread).
//!
//! `RUSTFORT_SIM_THREAD=0` (or `--sim-inline`) keeps the old behavior: ticks run inside the
//! frame with a time budget.

use rf_sim::Sim;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

/// Target real time per `run_ticks` call on the worker (latency for the UI to take the sim).
const CHUNK_S: f64 = 0.0005;

enum Msg {
    Run { sim: Box<Sim>, allow: u64 },
    Quit,
}

struct Worker {
    tx: Sender<Msg>,
    rx: Receiver<Box<Sim>>,
    give_back: Arc<AtomicBool>,
    busy_ns: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}

/// The game's simulation: either held by the UI thread (then it derefs to `Sim`) or running
/// on the worker between `release` and `acquire`.
pub struct SimHandle {
    sim: Option<Box<Sim>>,
    worker: Option<Worker>,
}

const _: () = {
    // The Sim must be movable to the worker thread.
    const fn assert_send<T: Send>() {}
    assert_send::<Sim>();
};

impl SimHandle {
    pub fn new(sim: Sim, threaded: bool) -> SimHandle {
        let worker = if threaded { spawn_worker() } else { None };
        SimHandle { sim: Some(Box::new(sim)), worker }
    }

    /// Ticks run on a worker thread.
    pub fn threaded(&self) -> bool {
        self.worker.is_some()
    }

    /// The UI holds the sim (always true between `acquire` and `release`).
    #[allow(dead_code)]
    pub fn held(&self) -> bool {
        self.sim.is_some()
    }

    /// Take the sim back from the worker (blocks for at most one tick chunk).
    pub fn acquire(&mut self) {
        if self.sim.is_some() {
            return;
        }
        let w = self.worker.as_ref().expect("sim handle without sim or worker");
        w.give_back.store(true, Ordering::Release);
        let sim = w.rx.recv().unwrap_or_else(|_| panic!("the simulation thread stopped (panicked?)"));
        w.give_back.store(false, Ordering::Release);
        self.sim = Some(sim);
    }

    /// Let the worker run up to `allow` ticks until the next `acquire`. Without a worker, or
    /// with nothing to run, the UI keeps the sim.
    pub fn release(&mut self, allow: u64) {
        let Some(w) = self.worker.as_ref() else { return };
        if allow == 0 {
            return;
        }
        if let Some(sim) = self.sim.take() {
            if let Err(e) = w.tx.send(Msg::Run { sim, allow }) {
                // The worker is gone: keep simulating on the UI thread.
                if let Msg::Run { sim, .. } = e.0 {
                    self.sim = Some(sim);
                }
                self.worker = None;
            }
        }
    }

    /// Wait until the worker has run its whole allowance (never use with `u64::MAX`).
    #[cfg(test)]
    pub fn finish(&mut self) {
        if self.sim.is_none() {
            let w = self.worker.as_ref().expect("sim handle without sim or worker");
            self.sim = Some(w.rx.recv().expect("the simulation thread stopped"));
        }
    }

    /// Total real time the worker spent running ticks (seconds).
    pub fn busy_secs(&self) -> f64 {
        self.worker.as_ref().map(|w| w.busy_ns.load(Ordering::Relaxed) as f64 * 1e-9).unwrap_or(0.0)
    }
}

impl Deref for SimHandle {
    type Target = Sim;
    fn deref(&self) -> &Sim {
        self.sim.as_deref().expect("the sim is on the worker thread (acquire it first)")
    }
}

impl DerefMut for SimHandle {
    fn deref_mut(&mut self) -> &mut Sim {
        self.sim.as_deref_mut().expect("the sim is on the worker thread (acquire it first)")
    }
}

impl Drop for SimHandle {
    fn drop(&mut self) {
        if self.worker.is_some() {
            self.acquire();
        }
        if let Some(mut w) = self.worker.take() {
            let _ = w.tx.send(Msg::Quit);
            if let Some(t) = w.thread.take() {
                let _ = t.join();
            }
        }
    }
}

fn spawn_worker() -> Option<Worker> {
    let (tx, wrx) = channel::<Msg>();
    let (wtx, rx) = channel::<Box<Sim>>();
    let give_back = Arc::new(AtomicBool::new(false));
    let busy_ns = Arc::new(AtomicU64::new(0));
    let (gb, busy) = (give_back.clone(), busy_ns.clone());
    let thread = std::thread::Builder::new()
        .name("rustfort-sim".into())
        .spawn(move || {
            let mut chunk: u64 = 8;
            while let Ok(msg) = wrx.recv() {
                let (mut sim, allow) = match msg {
                    Msg::Quit => break,
                    Msg::Run { sim, allow } => (sim, allow),
                };
                let mut left = allow;
                while left > 0 && !gb.load(Ordering::Acquire) {
                    let n = chunk.min(left).min(u32::MAX as u64);
                    let t = Instant::now();
                    sim.run_ticks(n as u32);
                    let dt = t.elapsed();
                    busy.fetch_add(dt.as_nanos() as u64, Ordering::Relaxed);
                    left -= n;
                    let per = dt.as_secs_f64() / n as f64;
                    chunk = if per > 0.0 { ((CHUNK_S / per) as u64).clamp(1, 5000) } else { 5000 };
                }
                if wtx.send(sim).is_err() {
                    break;
                }
            }
        })
        .ok()?;
    Some(Worker { tx, rx, give_back, busy_ns, thread: Some(thread) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_runs_exactly_the_allowance_and_hands_back() {
        let w = rf_core::World::synthetic(24, 24, 12);
        let mut h = SimHandle::new(Sim::new(w, 1), true);
        assert!(h.threaded() && h.held());
        h.release(40);
        assert!(!h.held());
        h.finish();
        assert_eq!(h.ticks(), 40);
        // An open-ended run stops when the UI takes the sim back.
        h.release(u64::MAX);
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.acquire();
        let t = h.ticks();
        assert!(t >= 40);
        h.run_ticks(1);
        assert_eq!(h.ticks(), t + 1);
    }

    #[test]
    fn same_result_as_inline() {
        let mk = || Sim::new(rf_core::World::synthetic(24, 24, 12), 7);
        let mut inline = SimHandle::new(mk(), false);
        inline.release(300); // no worker: keeps the sim
        assert!(inline.held());
        inline.run_ticks(300);
        let mut thr = SimHandle::new(mk(), true);
        for _ in 0..3 {
            thr.release(100);
            thr.finish();
        }
        assert_eq!(thr.ticks(), 300);
        let pos = |s: &Sim| s.world.units.iter().map(|u| (u.id, u.pos)).collect::<Vec<_>>();
        assert_eq!(pos(&inline), pos(&thr));
    }
}
