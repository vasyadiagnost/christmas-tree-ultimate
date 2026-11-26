//! A single, optional XAudio2 worker for the whole app. Rendering only publishes the latest desired sound;
//! device calls, voice cleanup and fades never run on the snow or UI threads.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{E_FAIL, E_INVALIDARG, HMODULE};
use windows::Win32::Media::Audio::XAudio2::{
    IXAudio2, IXAudio2MasteringVoice, IXAudio2SourceVoice, IXAudio2Voice, IXAudio2VoiceCallback, XAUDIO2_BUFFER, XAUDIO2_COMMIT_NOW,
    XAUDIO2_DEFAULT_SAMPLERATE, XAUDIO2_LOOP_INFINITE, XAUDIO2_USE_DEFAULT_PROCESSOR, XAUDIO2_VOICE_NOPITCH, XAUDIO2_VOICE_NOSAMPLESPLAYED,
    XAUDIO2_VOICE_STATE,
};
use windows::Win32::Media::Audio::{AudioCategory_GameEffects, WAVE_FORMAT_PCM, WAVEFORMATEX};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{Error, HRESULT, Interface, PCWSTR, Result, s, w};

use snowhush_core::christmas::TransportKind;

use crate::log;

const BELLS: &[u8] = include_bytes!("../assets/audio/santa-bells.wav");
const TRACTOR: &[u8] = include_bytes!("../assets/audio/tractor-snow.wav");
const FADE_SECONDS: f32 = 0.18;
const TICK: Duration = Duration::from_millis(10);
const STALE_SCENE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
pub struct SoundScene {
    pub kind: TransportKind,
    /// Identifies a particular appearance, including one restarted after a monitor/layout change.
    pub event_id: u64,
    /// Application and per-effect volume, including any distance/position envelope; never system volume.
    pub gain: f32,
    pub pan: f32,
}

#[derive(Clone, Copy)]
struct Desired {
    scene: Option<SoundScene>,
    updated: Instant,
    revision: u64,
    epoch: u64,
}

struct Shared {
    desired: Mutex<Desired>,
    changed: Condvar,
    /// A normal disappearance fades out; an explicit stop invalidates old snapshots and silences immediately.
    silence: AtomicBool,
    mute_epoch: AtomicU64,
    shutdown: AtomicBool,
    enabled: AtomicBool,
}

pub struct AudioController {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl AudioController {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            desired: Mutex::new(Desired { scene: None, updated: Instant::now(), revision: 0, epoch: 0 }),
            changed: Condvar::new(),
            silence: AtomicBool::new(true),
            mute_epoch: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        });
        let state = shared.clone();
        let worker = match thread::Builder::new().name("holiday audio".into()).spawn(move || {
            if let Err(error) = run(&state) {
                log!("audio disabled: {error}");
            }
            state.enabled.store(false, Ordering::Release);
        }) {
            Ok(worker) => Some(worker),
            Err(error) => {
                log!("audio worker unavailable: {error}");
                shared.enabled.store(false, Ordering::Release);
                None
            }
        };
        Self { shared, worker }
    }

    /// Publish once per visible scene/frame. A stalled renderer cannot leave a loop playing indefinitely.
    /// The mailbox holds one snapshot; contention drops one update rather than stalling a frame or growing a queue.
    pub fn update(&self, desired: Option<SoundScene>) {
        if !self.shared.enabled.load(Ordering::Acquire) {
            return;
        }
        let desired = desired.map(|scene| SoundScene {
            gain: if scene.gain.is_finite() { scene.gain.clamp(0.0, 1.0) } else { 0.0 },
            pan: if scene.pan.is_finite() { scene.pan.clamp(-1.0, 1.0) } else { 0.0 },
            ..scene
        });
        if desired.is_none() {
            // Even if the worker briefly owns the mailbox, a disappearance must still be observed.
            self.shared.silence.store(true, Ordering::Release);
        }
        if let Ok(mut latest) = self.shared.desired.try_lock() {
            *latest = Desired {
                scene: desired,
                updated: Instant::now(),
                revision: latest.revision.wrapping_add(1),
                epoch: self.shared.mute_epoch.load(Ordering::Acquire),
            };
            self.shared.silence.store(desired.is_none(), Ordering::Release);
            self.shared.changed.notify_one();
        } else if desired.is_none() {
            self.shared.changed.notify_one();
        }
    }

    /// Lock, sound-off, disabled decorations, renderer failure and quit use this immediate, priority mute.
    pub fn stop(&self) {
        self.shared.silence.store(true, Ordering::Release);
        self.shared.mute_epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.changed.notify_one();
    }
}

impl Drop for AudioController {
    fn drop(&mut self) {
        self.stop();
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.changed.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Playing {
    scene: SoundScene,
    envelope: f32,
    gain: f32,
    pan: f32,
    sent_gain: f32,
    sent_pan: f32,
}

fn run(shared: &Shared) -> Result<()> {
    // All interfaces are created and destroyed on this worker; no COM/XAudio2 handles cross threads.
    let _com = Com::new()?;
    let engine = Engine::new()?;
    log!("audio ready: XAudio2.9, two original embedded PCM loops");
    let mut playing: Option<Playing> = None;
    let mut epoch = shared.mute_epoch.load(Ordering::Acquire);
    let mut previous = Instant::now();
    while !shared.shutdown.load(Ordering::Acquire) {
        let requested = *shared.desired.lock().unwrap_or_else(|poison| poison.into_inner());
        let now = Instant::now();
        let mut dt = now.duration_since(previous).as_secs_f32();
        previous = now;
        let next_epoch = shared.mute_epoch.load(Ordering::Acquire);
        let silent = shared.silence.load(Ordering::Acquire);
        if next_epoch != epoch {
            engine.stop_all()?;
            playing = None;
            epoch = next_epoch;
        }
        let desired =
            if !silent && requested.epoch == epoch && now.duration_since(requested.updated) <= STALE_SCENE { requested.scene } else { None };
        if let Some(scene) = desired {
            let restart = playing.as_ref().is_none_or(|current| current.scene.kind != scene.kind || current.scene.event_id != scene.event_id);
            if restart {
                engine.stop_all()?;
                engine.start(scene.kind)?;
                playing = Some(Playing { scene, envelope: 0.0, gain: scene.gain, pan: scene.pan, sent_gain: -1.0, sent_pan: f32::INFINITY });
                // Time spent idle belongs to the previous scene, rather than to this sound's fade-in.
                dt = 0.0;
            }
        }
        if let Some(current) = &mut playing {
            if let Some(scene) = desired {
                current.scene = scene;
                current.envelope = (current.envelope + dt / FADE_SECONDS).min(1.0);
            } else {
                current.envelope = (current.envelope - dt / FADE_SECONDS).max(0.0);
            }
            let gain_ease = 1.0 - (-dt / 0.025).exp();
            let pan_ease = 1.0 - (-dt / 0.060).exp();
            current.gain += (current.scene.gain - current.gain) * gain_ease;
            current.pan += (current.scene.pan - current.pan) * pan_ease;
            let gain = current.gain * current.envelope;
            if (gain - current.sent_gain).abs() > 0.0001 {
                engine.volume(current.scene.kind, gain)?;
                current.sent_gain = gain;
            }
            if (current.pan - current.sent_pan).abs() > 0.002 {
                engine.pan(current.scene.kind, current.pan)?;
                current.sent_pan = current.pan;
            }
            if desired.is_none() && current.envelope <= 0.0 {
                engine.stop_all()?;
                playing = None;
            }
        }
        let wait = if playing.is_some() { TICK } else { Duration::from_millis(500) };
        let latest = shared.desired.lock().unwrap_or_else(|poison| poison.into_inner());
        if latest.revision == requested.revision
            && shared.mute_epoch.load(Ordering::Acquire) == epoch
            && shared.silence.load(Ordering::Acquire) == silent
            && !shared.shutdown.load(Ordering::Acquire)
        {
            drop(shared.changed.wait_timeout(latest, wait).unwrap_or_else(|poison| poison.into_inner()));
        }
    }
    engine.stop_all()
}

struct Com;

impl Com {
    fn new() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Voice interfaces are not IUnknown objects: each must be explicitly destroyed, once, before its mastering voice.
struct Voice(IXAudio2SourceVoice);

impl Drop for Voice {
    fn drop(&mut self) {
        unsafe { self.0.DestroyVoice() };
    }
}

struct Master(IXAudio2MasteringVoice);

impl Drop for Master {
    fn drop(&mut self) {
        unsafe { self.0.DestroyVoice() };
    }
}

struct Engine {
    // Field order is intentional: source voices drop before mastering, then the engine and its COM reference.
    voices: [Voice; 2],
    master: Master,
    engine: IXAudio2,
    waves: [Wave; 2],
    /// Keep the system DLL loaded until process exit; its audio thread must never execute an unloaded module.
    _module: HMODULE,
}

impl Engine {
    fn new() -> Result<Self> {
        type Create = unsafe extern "system" fn(*mut *mut c_void, u32, u32) -> HRESULT;
        unsafe {
            let module = LoadLibraryExW(w!("xaudio2_9.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)?;
            let procedure = GetProcAddress(module, s!("XAudio2Create")).ok_or_else(Error::from_thread)?;
            let create: Create = std::mem::transmute(procedure);
            let mut raw = std::ptr::null_mut();
            create(&mut raw, 0, XAUDIO2_USE_DEFAULT_PROCESSOR).ok()?;
            if raw.is_null() {
                return Err(Error::new(E_FAIL, "XAudio2Create returned no engine"));
            }
            let engine = IXAudio2::from_raw(raw);
            let mut master = None;
            engine.CreateMasteringVoice(&mut master, 2, XAUDIO2_DEFAULT_SAMPLERATE, 0, PCWSTR::null(), None, AudioCategory_GameEffects)?;
            let master = Master(master.ok_or_else(Error::empty)?);
            master.0.SetVolume(1.0, XAUDIO2_COMMIT_NOW)?;
            let waves = [Wave::read(BELLS)?, Wave::read(TRACTOR)?];
            let make_voice = |wave: &Wave| -> Result<Voice> {
                let mut voice = None;
                engine.CreateSourceVoice(&mut voice, &wave.format, XAUDIO2_VOICE_NOPITCH, 1.0, None::<&IXAudio2VoiceCallback>, None, None)?;
                Ok(Voice(voice.ok_or_else(Error::empty)?))
            };
            let voices = [make_voice(&waves[0])?, make_voice(&waves[1])?];
            Ok(Self { voices, master, engine, waves, _module: module })
        }
    }

    fn index(kind: TransportKind) -> usize {
        match kind {
            TransportKind::Santa => 0,
            TransportKind::Tractor => 1,
        }
    }

    fn start(&self, kind: TransportKind) -> Result<()> {
        let index = Self::index(kind);
        let buffer = XAUDIO2_BUFFER {
            AudioBytes: self.waves[index].pcm.len() as u32,
            pAudioData: self.waves[index].pcm.as_ptr(),
            LoopCount: XAUDIO2_LOOP_INFINITE,
            ..Default::default()
        };
        unsafe {
            self.voices[index].0.SetVolume(0.0, XAUDIO2_COMMIT_NOW)?;
            self.voices[index].0.SubmitSourceBuffer(&buffer, None)?;
            self.voices[index].0.Start(0, XAUDIO2_COMMIT_NOW)
        }
    }

    fn volume(&self, kind: TransportKind, volume: f32) -> Result<()> {
        unsafe { self.voices[Self::index(kind)].0.SetVolume(volume, XAUDIO2_COMMIT_NOW) }
    }

    fn pan(&self, kind: TransportKind, pan: f32) -> Result<()> {
        // Keep both speakers present even at the screen edges; no sudden, hard left/right jumps.
        let angle = (pan.clamp(-1.0, 1.0) * 0.65 + 1.0) * std::f32::consts::FRAC_PI_4;
        let matrix = [angle.cos(), angle.sin()];
        let master: &IXAudio2Voice = &self.master.0;
        unsafe { self.voices[Self::index(kind)].0.SetOutputMatrix(master, 1, 2, matrix.as_ptr(), XAUDIO2_COMMIT_NOW) }
    }

    fn stop_all(&self) -> Result<()> {
        unsafe {
            // Mute both first: draining an asynchronous Stop must not leave the other voice audible.
            for voice in &self.voices {
                voice.0.SetVolume(0.0, XAUDIO2_COMMIT_NOW)?;
                voice.0.Stop(0, XAUDIO2_COMMIT_NOW)?;
            }
            for voice in &self.voices {
                let deadline = Instant::now() + Duration::from_millis(150);
                loop {
                    voice.0.FlushSourceBuffers()?;
                    let mut state = XAUDIO2_VOICE_STATE::default();
                    voice.0.GetState(&mut state, XAUDIO2_VOICE_NOSAMPLESPLAYED);
                    if state.BuffersQueued == 0 {
                        break;
                    }
                    if Instant::now() >= deadline {
                        return Err(Error::new(E_FAIL, "XAudio2 voice did not finish stopping"));
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { self.engine.StopEngine() };
    }
}

struct Wave {
    format: WAVEFORMATEX,
    pcm: &'static [u8],
}

impl Wave {
    fn read(bytes: &'static [u8]) -> Result<Self> {
        let invalid = || Error::new(E_INVALIDARG, "Embedded audio must be a complete mono 16-bit PCM WAV");
        let u16_at = |at: usize| -> Option<u16> { Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?)) };
        let u32_at = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
        if bytes.get(..4) != Some(b"RIFF")
            || bytes.get(8..12) != Some(b"WAVE")
            || u32_at(4).and_then(|size| (size as usize).checked_add(8)) != Some(bytes.len())
        {
            return Err(invalid());
        }
        let mut format = None;
        let mut pcm = None;
        let mut at = 12usize;
        while at + 8 <= bytes.len() {
            let length = u32_at(at + 4).ok_or_else(invalid)? as usize;
            let end = at.checked_add(8).and_then(|start| start.checked_add(length)).ok_or_else(invalid)?;
            if end > bytes.len() {
                return Err(invalid());
            }
            match &bytes[at..at + 4] {
                b"fmt " => {
                    if format.is_some() || length < 16 || length == 17 {
                        return Err(invalid());
                    }
                    if u16_at(at + 8) != Some(WAVE_FORMAT_PCM as u16) || u16_at(at + 10) != Some(1) || u16_at(at + 22) != Some(16) {
                        return Err(invalid());
                    }
                    let rate = u32_at(at + 12).ok_or_else(invalid)?;
                    if !(8000..=192000).contains(&rate) || u32_at(at + 16) != Some(rate * 2) || u16_at(at + 20) != Some(2) {
                        return Err(invalid());
                    }
                    format = Some(WAVEFORMATEX {
                        wFormatTag: WAVE_FORMAT_PCM as u16,
                        nChannels: 1,
                        nSamplesPerSec: rate,
                        nAvgBytesPerSec: rate * 2,
                        nBlockAlign: 2,
                        wBitsPerSample: 16,
                        cbSize: 0,
                    });
                }
                b"data" => {
                    if pcm.is_some() || length == 0 || length % 2 != 0 {
                        return Err(invalid());
                    }
                    pcm = Some(&bytes[at + 8..end]);
                }
                _ => {}
            }
            at = end.checked_add(length & 1).ok_or_else(invalid)?;
        }
        if at != bytes.len() {
            return Err(invalid());
        }
        Ok(Self { format: format.ok_or_else(invalid)?, pcm: pcm.ok_or_else(invalid)? })
    }
}

#[cfg(test)]
mod tests {
    use super::{BELLS, TRACTOR, Wave};

    fn small_wave() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend(40u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(44100u32.to_le_bytes());
        bytes.extend(88200u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(4u32.to_le_bytes());
        bytes.extend([0u8; 4]);
        bytes
    }

    fn read_owned(mut bytes: Vec<u8>) -> windows::core::Result<Wave> {
        let length = bytes.len().saturating_sub(8) as u32;
        bytes[4..8].copy_from_slice(&length.to_le_bytes());
        // Production buffers are embedded static data. Tests leak only their tiny malformed header fixtures.
        Wave::read(Box::leak(bytes.into_boxed_slice()))
    }

    #[test]
    fn embedded_loops_have_audible_samples_and_exact_pcm_duration() {
        for (bytes, frames) in [(BELLS, 264600), (TRACTOR, 44100 * 8)] {
            let wave = Wave::read(bytes).expect("valid embedded WAV");
            let format = wave.format;
            let rate = format.nSamplesPerSec;
            let channels = format.nChannels;
            let bits = format.wBitsPerSample;
            assert_eq!((rate, channels, bits), (44100, 1, 16));
            assert_eq!(wave.pcm.len(), frames * 2);
            assert!(wave.pcm.chunks_exact(2).any(|sample| i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs() > 100));
        }
    }

    #[test]
    fn santa_bells_keep_the_user_recording_with_a_continuous_loop_boundary() {
        let wave = Wave::read(BELLS).expect("valid imported sleigh bells");
        let fingerprint = wave.pcm.iter().fold(14_695_981_039_346_656_037u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(1_099_511_628_211));
        assert_eq!(fingerprint, 0x1167_7649_b93d_1677);
        assert_eq!(wave.pcm.len(), 264600 * 2);
    }

    #[test]
    fn santa_bells_do_not_have_a_quiet_gap_inside_the_loop_or_at_its_wrap() {
        let wave = Wave::read(BELLS).expect("valid looped sleigh bells");
        let energy =
            |bytes: &[u8]| bytes.chunks_exact(2).map(|sample| (f64::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0).powi(2)).sum::<f64>();
        for (index, block) in wave.pcm.chunks_exact(441 * 2).enumerate() {
            assert!(energy(block) / 441.0 > 0.001f64.powi(2), "unexpected quiet 10 ms block at {index}");
        }
        let boundary = energy(&wave.pcm[wave.pcm.len() - 220 * 2..]) + energy(&wave.pcm[..221 * 2]);
        assert!(boundary / 441.0 > 0.001f64.powi(2), "quiet gap at the cyclic wrap");
    }

    #[test]
    fn every_truncated_prefix_is_rejected_without_reading_past_the_buffer() {
        let bytes: &'static [u8] = Box::leak(small_wave().into_boxed_slice());
        for end in 0..bytes.len() {
            assert!(Wave::read(&bytes[..end]).is_err(), "accepted a truncated header/data prefix at {end}");
        }
        assert!(Wave::read(bytes).is_ok());
    }

    #[test]
    fn non_pcm_stereo_wrong_bit_depth_and_inconsistent_rates_are_rejected() {
        for (offset, replacement) in [
            (20, 3u16.to_le_bytes().to_vec()), // IEEE float cannot be interpreted as PCM16.
            (22, 2u16.to_le_bytes().to_vec()),
            (24, 0u32.to_le_bytes().to_vec()),
            (28, 44100u32.to_le_bytes().to_vec()),
            (32, 1u16.to_le_bytes().to_vec()),
            (34, 8u16.to_le_bytes().to_vec()),
        ] {
            let mut bytes = small_wave();
            bytes[offset..offset + replacement.len()].copy_from_slice(&replacement);
            assert!(read_owned(bytes).is_err(), "accepted malformed format field at {offset}");
        }
    }

    #[test]
    fn corrupt_riff_and_out_of_bounds_data_lengths_are_rejected() {
        let mut bytes = small_wave();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Wave::read(Box::leak(bytes.into_boxed_slice())).is_err());
        for length in [0, 3, u32::MAX] {
            let mut bytes = small_wave();
            bytes[40..44].copy_from_slice(&length.to_le_bytes());
            assert!(read_owned(bytes).is_err(), "accepted PCM data length {length}");
        }
        for offset in [0, 8] {
            let mut bytes = small_wave();
            bytes[offset] = b'?';
            assert!(read_owned(bytes).is_err());
        }
    }

    #[test]
    fn odd_sized_ancillary_chunks_need_their_padding_and_a_complete_trailing_header() {
        let mut bytes = small_wave();
        bytes.splice(36..36, [b'J', b'U', b'N', b'K', 1, 0, 0, 0, 7, 0]);
        assert!(read_owned(bytes).is_ok());

        let mut bytes = small_wave();
        bytes.extend([b'J', b'U', b'N', b'K', 1, 0, 0, 0, 7]);
        assert!(read_owned(bytes).is_err(), "accepted an odd chunk without RIFF padding");

        let mut bytes = small_wave();
        bytes.extend(b"JUNK");
        assert!(read_owned(bytes).is_err(), "accepted an incomplete final chunk header");
    }

    #[test]
    fn ambiguous_duplicate_audio_chunks_are_rejected() {
        for duplicate in [small_wave()[12..36].to_vec(), small_wave()[36..48].to_vec()] {
            let mut bytes = small_wave();
            bytes.extend(duplicate);
            assert!(read_owned(bytes).is_err());
        }
    }

    #[test]
    #[ignore = "requires a Windows audio device; silent hardware smoke"]
    fn xaudio_loop_start_and_stop_stays_silent() -> windows::core::Result<()> {
        use super::{Com, Engine, TransportKind, XAUDIO2_COMMIT_NOW, XAUDIO2_VOICE_NOSAMPLESPLAYED, XAUDIO2_VOICE_STATE};

        let _com = Com::new()?;
        let engine = Engine::new()?;
        // This is our own mastering voice, not the Windows endpoint or the user's system volume.
        unsafe { engine.master.0.SetVolume(0.0, XAUDIO2_COMMIT_NOW)? };
        assert_eq!(unsafe { engine.master.0.GetVolume() }, 0.0);
        for (kind, pan) in [(TransportKind::Santa, -0.3), (TransportKind::Tractor, 0.3)] {
            engine.start(kind)?; // Production start submits the infinite loop at exactly zero volume.
            engine.volume(kind, 0.0)?;
            engine.pan(kind, pan)?;
            std::thread::sleep(std::time::Duration::from_millis(40));
            let voice = &engine.voices[Engine::index(kind)].0;
            let mut state = XAUDIO2_VOICE_STATE::default();
            unsafe { voice.GetState(&mut state, XAUDIO2_VOICE_NOSAMPLESPLAYED) };
            let queued = state.BuffersQueued;
            assert!(queued > 0, "the infinite loop must remain queued while its silent voice is started");
            assert_eq!(unsafe { voice.GetVolume() }, 0.0, "hardware smoke must stay silent");
            engine.stop_all()?;
            for voice in &engine.voices {
                let mut state = XAUDIO2_VOICE_STATE::default();
                unsafe { voice.0.GetState(&mut state, XAUDIO2_VOICE_NOSAMPLESPLAYED) };
                let queued = state.BuffersQueued;
                assert_eq!(queued, 0, "Stop/Flush must drain even an infinite loop");
                assert_eq!(unsafe { voice.0.GetVolume() }, 0.0);
            }
        }
        Ok(())
    }
}
