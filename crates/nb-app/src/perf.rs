//! Mesure des temps de frame, activée par la variable d'environnement `NB_PERF=1`.
//!
//! Une ligne de synthèse par seconde (images/s, durée moyenne / maximale d'une frame, temps par
//! phase) et une ligne pour chaque frame de plus de 25 ms, avec la phase responsable : de quoi
//! retrouver d'où vient une saccade sans profileur externe. Sans la variable, tout est inerte.

use std::time::{Duration, Instant};

pub struct Perf {
    on: bool,
    since: Instant,
    frames: u32,
    sum_frame: f64,
    max_frame: f64,
    sum_tick: f64,
    sum_stream: f64,
    sum_mesh: f64,
    sum_render: f64,
    max_render: f64,
    spikes: u32,
    // Phases de la frame en cours (ms).
    frame_ms: f64,
    tick_ms: f64,
    stream_ms: f64,
    mesh_ms: f64,
    render_ms: f64,
    n_ticks: u32,
    backlog: usize,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl Perf {
    pub fn from_env() -> Self {
        let on = std::env::var("NB_PERF").is_ok_and(|v| v != "0" && !v.is_empty());
        if on {
            eprintln!("[perf] mesures activées (NB_PERF=1)");
        }
        Self {
            on,
            since: Instant::now(),
            frames: 0,
            sum_frame: 0.0,
            max_frame: 0.0,
            sum_tick: 0.0,
            sum_stream: 0.0,
            sum_mesh: 0.0,
            sum_render: 0.0,
            max_render: 0.0,
            spikes: 0,
            frame_ms: 0.0,
            tick_ms: 0.0,
            stream_ms: 0.0,
            mesh_ms: 0.0,
            render_ms: 0.0,
            n_ticks: 0,
            backlog: 0,
        }
    }

    /// Début de frame : `frame` = durée de la frame précédente en secondes.
    pub fn begin(&mut self, frame: f64) {
        self.frame_ms = frame * 1000.0;
    }

    pub fn ticks(&mut self, n: u32, ticks: Duration, stream: Duration) {
        self.n_ticks = n;
        self.tick_ms = ms(ticks);
        self.stream_ms = ms(stream);
    }

    pub fn meshes(&mut self, upload: Duration, backlog: usize) {
        self.mesh_ms = ms(upload);
        self.backlog = backlog;
    }

    pub fn render(&mut self, render: Duration) {
        self.render_ms = ms(render);
    }

    /// Fin de frame : cumule, et affiche la synthèse chaque seconde.
    pub fn end(&mut self) {
        if !self.on {
            return;
        }
        self.frames += 1;
        self.sum_frame += self.frame_ms;
        self.max_frame = self.max_frame.max(self.frame_ms);
        self.sum_tick += self.tick_ms;
        self.sum_stream += self.stream_ms;
        self.sum_mesh += self.mesh_ms;
        self.sum_render += self.render_ms;
        self.max_render = self.max_render.max(self.render_ms);
        if self.frame_ms > 25.0 {
            self.spikes += 1;
            eprintln!(
                "[perf] SACCADE {:.1} ms | ticks {} ({:.1} ms) | streaming {:.1} ms | envoi maillages {:.1} ms (reste {}) | rendu {:.1} ms",
                self.frame_ms,
                self.n_ticks,
                self.tick_ms,
                self.stream_ms,
                self.mesh_ms,
                self.backlog,
                self.render_ms
            );
        }
        if self.since.elapsed() >= Duration::from_secs(1) {
            let n = f64::from(self.frames.max(1));
            eprintln!(
                "[perf] {:>4} i/s | frame moy {:.1} max {:.1} ms | ticks {:.2} | streaming {:.2} | envoi {:.2} | rendu moy {:.2} max {:.2} | saccades {} | file {}",
                self.frames,
                self.sum_frame / n,
                self.max_frame,
                self.sum_tick / n,
                self.sum_stream / n,
                self.sum_mesh / n,
                self.sum_render / n,
                self.max_render,
                self.spikes,
                self.backlog
            );
            self.since = Instant::now();
            self.frames = 0;
            self.sum_frame = 0.0;
            self.max_frame = 0.0;
            self.sum_tick = 0.0;
            self.sum_stream = 0.0;
            self.sum_mesh = 0.0;
            self.sum_render = 0.0;
            self.max_render = 0.0;
            self.spikes = 0;
        }
    }
}
