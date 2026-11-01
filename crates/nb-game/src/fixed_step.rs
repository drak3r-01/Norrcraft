//! Boucle à pas fixe (`GameLoop.java`), accumulateur borné (écart appliqué, spec §1).

/// Pas de temps fixe : `1.0 / 60.0` (f64, comme le Java).
pub const TIME_STEP: f64 = 1.0 / 60.0;

/// Borne par défaut de la durée d'une frame (0,25 s → au plus 15 ticks par frame).
pub const DEFAULT_MAX_FRAME: f64 = 0.25;

/// Accumulateur de la boucle fixe.
///
/// `advance(frame)` renvoie le nombre de `update(step)` à exécuter avant le rendu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedStep {
    step: f64,
    accumulator: f64,
    /// `None` = comportement Java (aucune borne).
    max_frame: Option<f64>,
}

impl Default for FixedStep {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedStep {
    /// 60 Hz, frame bornée à [`DEFAULT_MAX_FRAME`].
    pub fn new() -> Self {
        Self::with(TIME_STEP, Some(DEFAULT_MAX_FRAME))
    }

    /// Comportement exact du Java : aucune borne (spirale de la mort possible).
    pub fn java() -> Self {
        Self::with(TIME_STEP, None)
    }

    pub fn with(step: f64, max_frame: Option<f64>) -> Self {
        assert!(step > 0.0, "pas de temps nul");
        Self {
            step,
            accumulator: 0.0,
            max_frame,
        }
    }

    pub fn step(&self) -> f64 {
        self.step
    }

    /// Reliquat non consommé (jamais utilisé pour interpoler, comme le Java).
    pub fn accumulator(&self) -> f64 {
        self.accumulator
    }

    /// Ajoute la durée d'une frame et renvoie le nombre de ticks à exécuter.
    /// Une durée négative ou non finie compte pour 0.
    pub fn advance(&mut self, frame_time: f64) -> u32 {
        let mut frame = if frame_time.is_finite() {
            frame_time.max(0.0)
        } else {
            0.0
        };
        if let Some(max) = self.max_frame {
            frame = frame.min(max);
        }
        self.accumulator += frame;
        let mut n = 0;
        // Même soustraction répétée que le Java (`accumulator -= timeStep`).
        while self.accumulator >= self.step {
            self.accumulator -= self.step;
            n += 1;
        }
        n
    }

    /// Variante qui appelle `update(step)` `n` fois.
    pub fn run(&mut self, frame_time: f64, mut update: impl FnMut(f64)) -> u32 {
        let n = self.advance(frame_time);
        for _ in 0..n {
            update(self.step);
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_multiples_and_remainder() {
        let mut l = FixedStep::new();
        assert_eq!(l.advance(TIME_STEP * 0.5), 0);
        assert_eq!(l.advance(TIME_STEP * 0.5), 1);
        assert!(l.accumulator() < 1e-12);
        assert_eq!(l.advance(TIME_STEP * 3.0 + TIME_STEP * 0.25), 3);
        assert!((l.accumulator() - TIME_STEP * 0.25).abs() < 1e-9);
    }

    #[test]
    fn long_run_does_not_drift() {
        // 600 frames de 1/120 s → 300 ticks (à une unité près à cause des flottants).
        let mut l = FixedStep::new();
        let n: u32 = (0..600).map(|_| l.advance(1.0 / 120.0)).sum();
        assert!((299..=300).contains(&n), "{n}");
    }

    #[test]
    fn frame_is_clamped() {
        let mut l = FixedStep::new();
        // 0,25 s = 15 pas à l'erreur d'arrondi près de la soustraction répétée.
        assert!((14..=15).contains(&l.advance(10.0)));
        let mut j = FixedStep::java();
        assert!((59..=60).contains(&j.advance(1.0)));
        assert!(j.advance(9.0) >= 539);
    }

    #[test]
    fn bad_frame_times_are_ignored() {
        let mut l = FixedStep::new();
        assert_eq!(l.advance(-1.0), 0);
        assert_eq!(l.advance(f64::NAN), 0);
        assert_eq!(l.advance(f64::INFINITY), 0);
        assert_eq!(l.accumulator(), 0.0);
    }

    #[test]
    fn run_calls_update_with_step() {
        let mut l = FixedStep::new();
        let mut calls = Vec::new();
        assert_eq!(l.run(TIME_STEP * 2.5, |dt| calls.push(dt)), 2);
        assert_eq!(calls, vec![TIME_STEP, TIME_STEP]);
    }
}
