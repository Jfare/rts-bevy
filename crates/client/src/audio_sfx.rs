use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Event)]
#[allow(dead_code)]
pub enum SoundEffect {
    Gunshot,
    SiegeTankShot,
    Explosion,
    LaserMining,
    Stimpack,
    SiegeModeToggle,
    BuildPlaced,
    UnitTrained,
    OrderIssued,
    MarineSelect,
    TankSelect,
    WorkerSelect,
    BaseUnderAttack,
    SupplyBlocked,
    Victory,
    Defeat,
}

pub struct AudioSfxPlugin;

impl Plugin for AudioSfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SoundEffect>()
            .add_systems(Update, process_sound_effects);
    }
}

fn process_sound_effects(mut sound_events: EventReader<SoundEffect>) {
    for event in sound_events.read() {
        play_sound(*event);
    }
}

pub fn play_sound(sfx: SoundEffect) {
    #[cfg(target_arch = "wasm32")]
    {
        play_synth_audio_wasm(sfx);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = sfx;
    }
}

#[cfg(target_arch = "wasm32")]
fn play_synth_audio_wasm(sfx: SoundEffect) {
    let js_code = match sfx {
        SoundEffect::Gunshot => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sawtooth';
                    osc.frequency.setValueAtTime(880, ctx.currentTime);
                    osc.frequency.exponentialRampToValueAtTime(110, ctx.currentTime + 0.08);
                    gain.gain.setValueAtTime(0.16, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.08);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.09);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::SiegeTankShot => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const sub = ctx.createOscillator();
                    const gain = ctx.createGain();
                    const subGain = ctx.createGain();
                    osc.type = 'sawtooth';
                    osc.frequency.setValueAtTime(240, ctx.currentTime);
                    osc.frequency.exponentialRampToValueAtTime(30, ctx.currentTime + 0.35);
                    gain.gain.setValueAtTime(0.35, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.35);
                    sub.type = 'sine';
                    sub.frequency.setValueAtTime(90, ctx.currentTime);
                    sub.frequency.exponentialRampToValueAtTime(20, ctx.currentTime + 0.45);
                    subGain.gain.setValueAtTime(0.40, ctx.currentTime);
                    subGain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.45);
                    osc.connect(gain);
                    sub.connect(subGain);
                    gain.connect(ctx.destination);
                    subGain.connect(ctx.destination);
                    osc.start();
                    sub.start();
                    osc.stop(ctx.currentTime + 0.36);
                    sub.stop(ctx.currentTime + 0.46);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::Explosion => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const bufferSize = ctx.sampleRate * 0.4;
                    const buffer = ctx.createBuffer(1, bufferSize, ctx.sampleRate);
                    const data = buffer.getChannelData(0);
                    for (let i = 0; i < bufferSize; i++) {
                        data[i] = (Math.random() * 2 - 1) * Math.exp(-i / (ctx.sampleRate * 0.12));
                    }
                    const noise = ctx.createBufferSource();
                    noise.buffer = buffer;
                    const filter = ctx.createBiquadFilter();
                    filter.type = 'lowpass';
                    filter.frequency.setValueAtTime(450, ctx.currentTime);
                    filter.frequency.linearRampToValueAtTime(80, ctx.currentTime + 0.4);
                    const gain = ctx.createGain();
                    gain.gain.setValueAtTime(0.32, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.4);
                    noise.connect(filter);
                    filter.connect(gain);
                    gain.connect(ctx.destination);
                    noise.start();
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::LaserMining => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sine';
                    osc.frequency.setValueAtTime(1400, ctx.currentTime);
                    gain.gain.setValueAtTime(0.04, ctx.currentTime);
                    gain.gain.linearRampToValueAtTime(0.001, ctx.currentTime + 0.06);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.06);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::Stimpack => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'triangle';
                    osc.frequency.setValueAtTime(400, ctx.currentTime);
                    osc.frequency.linearRampToValueAtTime(1800, ctx.currentTime + 0.12);
                    gain.gain.setValueAtTime(0.20, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.14);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.15);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::SiegeModeToggle => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sawtooth';
                    osc.frequency.setValueAtTime(120, ctx.currentTime);
                    osc.frequency.linearRampToValueAtTime(320, ctx.currentTime + 0.20);
                    gain.gain.setValueAtTime(0.18, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.22);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.23);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::BuildPlaced => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'triangle';
                    osc.frequency.setValueAtTime(180, ctx.currentTime);
                    osc.frequency.exponentialRampToValueAtTime(60, ctx.currentTime + 0.15);
                    gain.gain.setValueAtTime(0.22, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.15);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.16);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::UnitTrained => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sine';
                    osc.frequency.setValueAtTime(523.25, ctx.currentTime);
                    osc.frequency.setValueAtTime(659.25, ctx.currentTime + 0.07);
                    gain.gain.setValueAtTime(0.12, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.18);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.19);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::OrderIssued => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sine';
                    osc.frequency.setValueAtTime(750, ctx.currentTime);
                    gain.gain.setValueAtTime(0.07, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.05);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.05);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::MarineSelect => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sine';
                    osc.frequency.setValueAtTime(580, ctx.currentTime);
                    osc.frequency.setValueAtTime(880, ctx.currentTime + 0.04);
                    gain.gain.setValueAtTime(0.08, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.10);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.11);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::TankSelect => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sawtooth';
                    osc.frequency.setValueAtTime(95, ctx.currentTime);
                    osc.frequency.exponentialRampToValueAtTime(140, ctx.currentTime + 0.10);
                    gain.gain.setValueAtTime(0.12, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.12);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.13);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::WorkerSelect => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'sine';
                    osc.frequency.setValueAtTime(440, ctx.currentTime);
                    osc.frequency.setValueAtTime(660, ctx.currentTime + 0.05);
                    gain.gain.setValueAtTime(0.07, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.11);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.12);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::BaseUnderAttack => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    [880, 660, 880].forEach((freq, i) => {
                        const osc = ctx.createOscillator();
                        const gain = ctx.createGain();
                        osc.type = 'sawtooth';
                        osc.frequency.value = freq;
                        const t = ctx.currentTime + i * 0.08;
                        gain.gain.setValueAtTime(0.15, t);
                        gain.gain.exponentialRampToValueAtTime(0.001, t + 0.07);
                        osc.connect(gain);
                        gain.connect(ctx.destination);
                        osc.start(t);
                        osc.stop(t + 0.08);
                    });
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::SupplyBlocked => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    const osc = ctx.createOscillator();
                    const gain = ctx.createGain();
                    osc.type = 'square';
                    osc.frequency.setValueAtTime(140, ctx.currentTime);
                    gain.gain.setValueAtTime(0.12, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.14);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.15);
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::Victory => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    [523.25, 659.25, 783.99, 1046.50].forEach((freq, i) => {
                        const osc = ctx.createOscillator();
                        const gain = ctx.createGain();
                        osc.type = 'sine';
                        osc.frequency.value = freq;
                        const t = ctx.currentTime + i * 0.12;
                        gain.gain.setValueAtTime(0.15, t);
                        gain.gain.exponentialRampToValueAtTime(0.001, t + 0.35);
                        osc.connect(gain);
                        gain.connect(ctx.destination);
                        osc.start(t);
                        osc.stop(t + 0.36);
                    });
                } catch (e) {}
            })()
            "#
        }
        SoundEffect::Defeat => {
            r#"
            (function() {
                try {
                    const ctx = window._rts_audio_ctx || (window._rts_audio_ctx = new (window.AudioContext || window.webkitAudioContext)());
                    if (ctx.state === 'suspended') ctx.resume();
                    [392.0, 349.23, 311.13, 261.63].forEach((freq, i) => {
                        const osc = ctx.createOscillator();
                        const gain = ctx.createGain();
                        osc.type = 'sawtooth';
                        osc.frequency.value = freq;
                        const t = ctx.currentTime + i * 0.18;
                        gain.gain.setValueAtTime(0.15, t);
                        gain.gain.exponentialRampToValueAtTime(0.001, t + 0.45);
                        osc.connect(gain);
                        gain.connect(ctx.destination);
                        osc.start(t);
                        osc.stop(t + 0.46);
                    });
                } catch (e) {}
            })()
            "#
        }
    };

    let _ = js_sys::eval(js_code);
}
