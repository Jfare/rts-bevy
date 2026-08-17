use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Event)]
#[allow(dead_code)]
pub enum SoundEffect {
    Gunshot,
    LaserMining,
    BuildPlaced,
    UnitTrained,
    OrderIssued,
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
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        fn log(s: &str);
    }

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
                    gain.gain.setValueAtTime(0.18, ctx.currentTime);
                    gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.08);
                    osc.connect(gain);
                    gain.connect(ctx.destination);
                    osc.start();
                    osc.stop(ctx.currentTime + 0.09);
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
