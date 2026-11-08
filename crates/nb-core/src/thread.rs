//! Utilitaires de threads de fond.

/// Abaisse la priorité du thread appelant (threads de génération, de lumière et de maillage).
///
/// Sans cela, sur un processeur à peu de cœurs, ces threads (plus nombreux que les cœurs)
/// volent du temps au thread principal qui prépare et soumet chaque image : saccades à chaque
/// arrivée de chunks. Le travail de fond passe après le rendu, jamais devant.
///
/// Windows : `THREAD_PRIORITY_BELOW_NORMAL`. Ailleurs : sans effet.
pub fn lower_priority() {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetCurrentThread() -> isize;
            fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        // SAFETY : appels Win32 sur le pseudo-handle du thread courant, sans pointeur.
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

/// Nombre de threads d'un pool de fond : la moitié des cœurs, entre 1 et 4. Au-delà, les
/// trois pools (génération, lumière, maillage) dépassent largement le nombre de cœurs sans
/// rien accélérer : un chunk se génère en ~0,3 ms et se maille en ~0,9 ms.
pub fn background_pool_size() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cpus / 2).clamp(1, 4)
}
