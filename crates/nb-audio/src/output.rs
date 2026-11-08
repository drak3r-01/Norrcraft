//! Sortie matérielle. Sous Windows : `waveOut` (winmm.dll, présente sur tout système, aucune
//! DLL à embarquer) avec quelques petits tampons rendus par un thread dédié. Ailleurs : pas de
//! périphérique (le moteur reste désactivé, le jeu fonctionne sans son).

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::mixer::Mixer;

/// Fréquence de sortie demandée au système.
pub const OUTPUT_RATE: u32 = 44_100;

/// Convertit un tampon flottant en PCM 16 bits.
pub fn to_i16(src: &[f32], dst: &mut [i16]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
    }
}

/// Démarre le thread de sortie. Le mixeur y est déplacé.
#[cfg(windows)]
pub fn spawn(mixer: Mixer, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>, String> {
    win::spawn(mixer, stop)
}

#[cfg(not(windows))]
pub fn spawn(_mixer: Mixer, _stop: Arc<AtomicBool>) -> Result<JoinHandle<()>, String> {
    Err("pas de périphérique audio sur cette plateforme".into())
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use super::{to_i16, OUTPUT_RATE};
    use crate::mixer::Mixer;

    type Hwaveout = *mut c_void;

    #[repr(C)]
    struct WaveFormatEx {
        format_tag: u16,
        channels: u16,
        samples_per_sec: u32,
        avg_bytes_per_sec: u32,
        block_align: u16,
        bits_per_sample: u16,
        cb_size: u16,
    }

    #[repr(C)]
    struct WaveHdr {
        data: *mut u8,
        buffer_length: u32,
        bytes_recorded: u32,
        user: usize,
        flags: u32,
        loops: u32,
        next: *mut WaveHdr,
        reserved: usize,
    }

    const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
    const WAVE_FORMAT_PCM: u16 = 1;
    const WHDR_DONE: u32 = 1;
    const MMSYSERR_NOERROR: u32 = 0;

    #[link(name = "winmm")]
    extern "system" {
        fn waveOutOpen(
            out: *mut Hwaveout,
            device: u32,
            format: *const WaveFormatEx,
            callback: usize,
            instance: usize,
            flags: u32,
        ) -> u32;
        fn waveOutPrepareHeader(h: Hwaveout, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutUnprepareHeader(h: Hwaveout, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutWrite(h: Hwaveout, hdr: *mut WaveHdr, size: u32) -> u32;
        fn waveOutReset(h: Hwaveout) -> u32;
        fn waveOutClose(h: Hwaveout) -> u32;
    }

    /// Images (frames) par tampon : ≈ 11,6 ms.
    const FRAMES: usize = 512;
    const BUFFERS: usize = 4;

    /// Ressources système possédées par le thread de sortie.
    struct Device {
        handle: Hwaveout,
        // Les en-têtes et les données doivent garder une adresse stable pendant toute la vie du
        // périphérique : tout est boxé et jamais redimensionné.
        headers: Vec<Box<WaveHdr>>,
        data: Vec<Box<[i16]>>,
        queued: [bool; BUFFERS],
    }

    impl Device {
        fn open() -> Result<Self, String> {
            let fmt = WaveFormatEx {
                format_tag: WAVE_FORMAT_PCM,
                channels: 2,
                samples_per_sec: OUTPUT_RATE,
                avg_bytes_per_sec: OUTPUT_RATE * 4,
                block_align: 4,
                bits_per_sample: 16,
                cb_size: 0,
            };
            let mut handle: Hwaveout = std::ptr::null_mut();
            // SAFETY: appel FFI winmm avec des pointeurs valides ; CALLBACK_NULL (0) = sondage.
            let r = unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &fmt, 0, 0, 0) };
            if r != MMSYSERR_NOERROR {
                return Err(format!("waveOutOpen a échoué (code {r})"));
            }
            let mut headers = Vec::new();
            let mut data: Vec<Box<[i16]>> = Vec::new();
            for _ in 0..BUFFERS {
                data.push(vec![0i16; FRAMES * 2].into_boxed_slice());
            }
            for d in &mut data {
                let mut h = Box::new(WaveHdr {
                    data: d.as_mut_ptr().cast(),
                    buffer_length: (FRAMES * 4) as u32,
                    bytes_recorded: 0,
                    user: 0,
                    flags: 0,
                    loops: 0,
                    next: std::ptr::null_mut(),
                    reserved: 0,
                });
                // SAFETY: `h` pointe vers des données boxées qui survivent au périphérique.
                let r = unsafe {
                    waveOutPrepareHeader(handle, &mut *h, std::mem::size_of::<WaveHdr>() as u32)
                };
                if r != MMSYSERR_NOERROR {
                    // SAFETY: handle ouvert ci-dessus.
                    unsafe { waveOutClose(handle) };
                    return Err(format!("waveOutPrepareHeader a échoué (code {r})"));
                }
                headers.push(h);
            }
            Ok(Self {
                handle,
                headers,
                data,
                queued: [false; BUFFERS],
            })
        }

        /// Remplit et met en file tous les tampons terminés (ou jamais envoyés).
        fn pump(&mut self, mixer: &mut Mixer, scratch: &mut [f32]) -> bool {
            let mut any = false;
            for i in 0..BUFFERS {
                let done = !self.queued[i] || self.headers[i].flags & WHDR_DONE != 0;
                if !done {
                    continue;
                }
                mixer.render(scratch);
                to_i16(scratch, &mut self.data[i]);
                // SAFETY: en-tête préparé, tampon vivant, non en cours de lecture (DONE).
                let r = unsafe {
                    waveOutWrite(
                        self.handle,
                        &mut *self.headers[i],
                        std::mem::size_of::<WaveHdr>() as u32,
                    )
                };
                self.queued[i] = r == MMSYSERR_NOERROR;
                any |= self.queued[i];
            }
            any
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            // SAFETY: handle valide ; `Reset` rend tous les tampons avant de les libérer.
            unsafe {
                waveOutReset(self.handle);
                for h in &mut self.headers {
                    waveOutUnprepareHeader(
                        self.handle,
                        &mut **h,
                        std::mem::size_of::<WaveHdr>() as u32,
                    );
                }
                waveOutClose(self.handle);
            }
        }
    }

    pub fn spawn(mut mixer: Mixer, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let handle = std::thread::Builder::new()
            .name("nb-audio".into())
            .spawn(move || {
                let mut dev = match Device::open() {
                    Ok(d) => {
                        let _ = ready_tx.send(Ok(()));
                        d
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mut scratch = vec![0.0f32; FRAMES * 2];
                while !stop.load(Ordering::Relaxed) {
                    if !dev.pump(&mut mixer, &mut scratch) {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(handle),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err("le périphérique audio ne répond pas".into()),
        }
    }
}
